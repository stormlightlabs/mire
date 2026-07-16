package model

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestOpenAICompatibleNonStreamingStructuredOutputAndProvenance(t *testing.T) {
	t.Parallel()
	var authorization string
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/v1/chat/completions" {
			t.Fatalf("path = %q", request.URL.Path)
		}
		authorization = request.Header.Get("Authorization")
		var payload map[string]any
		if err := json.NewDecoder(request.Body).Decode(&payload); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if payload["model"] != "fixture-model" || payload["stream"] != false {
			t.Fatalf("request model/stream = %#v/%#v", payload["model"], payload["stream"])
		}
		if format, ok := payload["response_format"].(map[string]any); !ok || format["type"] != "json_object" {
			t.Fatalf("response format = %#v", payload["response_format"])
		}
		if _, ok := payload["tools"].([]any); !ok {
			t.Fatalf("tools = %#v", payload["tools"])
		}
		response.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(
			response,
			`{"choices":[{"message":{"content":"{\"ok\":true}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}`,
		)
	}))
	defer server.Close()

	adapter, err := NewOpenAICompat(
		RoleConfig{
			Provider:      ProviderOpenAICompatible,
			BaseURL:       server.URL + "/v1",
			Model:         "fixture-model",
			CredentialRef: "test",
			Timeout:       time.Second,
		},
		testCredentials("secret-token"),
		server.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	result, err := adapter.Complete(context.Background(), modelRequest())
	if err != nil {
		t.Fatalf("Complete() error = %v", err)
	}
	if string(result.Output) != `{"ok":true}` || result.Usage.TotalTokens != 7 || result.FinishReason != "completed" {
		t.Fatalf("response = %#v", result)
	}
	if authorization != "Bearer secret-token" {
		t.Fatalf("authorization = %q", authorization)
	}
	metadata := adapter.Metadata()
	if metadata.Adapter != string(ProviderOpenAICompatible) || metadata.Protocol != openAIProtocolVersion ||
		metadata.Model != "fixture-model" ||
		len(metadata.Redactions) != 1 {
		t.Fatalf("metadata = %#v", metadata)
	}
}

func TestOpenAICompatibleStreamingUsageAndToolArguments(t *testing.T) {
	t.Parallel()
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Type", "text/event-stream")
		writeSSEPayload(
			response,
			map[string]any{"choices": []any{map[string]any{"delta": map[string]any{"content": `{"ok":`}}}},
		)
		writeSSEPayload(
			response,
			map[string]any{
				"choices": []any{map[string]any{"delta": map[string]any{"content": "true}"}, "finish_reason": "stop"}},
			},
		)
		writeSSEPayload(
			response,
			map[string]any{
				"choices": []any{},
				"usage":   map[string]any{"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7},
			},
		)
		_, _ = io.WriteString(response, "data: [DONE]\n\n")
	}))
	defer server.Close()
	config := RoleConfig{
		Provider: ProviderOpenAICompatible,
		BaseURL:  server.URL,
		Model:    "fixture",
		Stream:   true,
		Timeout:  time.Second,
	}
	adapter, err := NewOpenAICompat(config, nil, server.Client())
	if err != nil {
		t.Fatal(err)
	}
	result, err := adapter.Complete(context.Background(), modelRequest())
	if err != nil {
		t.Fatalf("Complete() error = %v", err)
	}
	if string(result.Output) != `{"ok":true}` || result.Usage.InputTokens != 5 || result.FinishReason != "completed" {
		t.Fatalf("response = %#v", result)
	}
}

func TestOpenAICompatibleRetriesRateLimitAndRedactsProviderError(t *testing.T) {
	t.Parallel()
	var calls atomic.Int32
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if calls.Add(1) == 1 {
			response.Header().Set("Retry-After", "0")
			response.WriteHeader(http.StatusTooManyRequests)
			_, _ = io.WriteString(response, `{"error":{"code":"rate_limit","message":"secret-token was rejected"}}`)
			return
		}
		_, _ = io.WriteString(response, `{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}`)
	}))
	defer server.Close()
	config := RoleConfig{
		Provider:      ProviderOpenAICompatible,
		BaseURL:       server.URL,
		Model:         "fixture",
		CredentialRef: "test",
		Timeout:       time.Second,
		Retry:         RetryPolicy{MaxAttempts: 2, InitialDelay: time.Millisecond, MaxDelay: time.Millisecond},
	}
	adapter, err := NewOpenAICompat(config, testCredentials("secret-token"), server.Client())
	if err != nil {
		t.Fatal(err)
	}
	result, err := adapter.Complete(context.Background(), modelRequest())
	if err != nil || string(result.Output) != "ok" || calls.Load() != 2 {
		t.Fatalf("result=%#v err=%v calls=%d", result, err, calls.Load())
	}

	errorServer := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.WriteHeader(http.StatusUnauthorized)
		_, _ = io.WriteString(response, `{"message":"secret-token"}`)
	}))
	defer errorServer.Close()
	adapter, err = NewOpenAICompat(
		RoleConfig{
			Provider:      ProviderOpenAICompatible,
			BaseURL:       errorServer.URL,
			Model:         "fixture",
			CredentialRef: "test",
			Timeout:       time.Second,
		},
		testCredentials("secret-token"),
		errorServer.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	_, err = adapter.Complete(context.Background(), modelRequest())
	if err == nil || strings.Contains(err.Error(), "secret-token") || !strings.Contains(err.Error(), "status 401") {
		t.Fatalf("redaction error = %v", err)
	}
}

func TestOpenAICompatibleRejectsIncompleteStreamAndEnforcesBudget(t *testing.T) {
	t.Parallel()
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Type", "text/event-stream")
		_, _ = io.WriteString(response, "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n")
	}))
	defer server.Close()
	adapter, err := NewOpenAICompat(
		RoleConfig{
			Provider: ProviderOpenAICompatible,
			BaseURL:  server.URL,
			Model:    "fixture",
			Stream:   true,
			Timeout:  time.Second,
		},
		nil,
		server.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	_, err = adapter.Complete(context.Background(), modelRequest())
	var streamErr *MalformedStreamError
	if !errors.As(err, &streamErr) || !strings.Contains(err.Error(), "[DONE]") {
		t.Fatalf("incomplete stream error = %v", err)
	}

	budgetServer := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		_, _ = io.WriteString(response, `{"choices":[{"message":{"content":"too long"},"finish_reason":"stop"}]}`)
	}))
	defer budgetServer.Close()
	adapter, err = NewOpenAICompat(
		RoleConfig{
			Provider: ProviderOpenAICompatible,
			BaseURL:  budgetServer.URL,
			Model:    "fixture",
			Timeout:  time.Second,
			Budget:   Budget{MaxOutputBytes: 3},
		},
		nil,
		budgetServer.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	_, err = adapter.Complete(context.Background(), modelRequest())
	var budgetErr *BudgetError
	if !errors.As(err, &budgetErr) || budgetErr.Kind != "output bytes" {
		t.Fatalf("budget error = %v", err)
	}
}

func TestOpenAICompatibleCapabilitiesAndContextCancellation(t *testing.T) {
	t.Parallel()
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/models" {
			response.WriteHeader(http.StatusNotFound)
			return
		}
		t.Fatalf("unexpected request path %q", request.URL.Path)
	}))
	defer server.Close()
	adapter, err := NewOpenAICompat(
		RoleConfig{Provider: ProviderOpenAICompatible, BaseURL: server.URL, Model: "fixture", Timeout: time.Second},
		nil,
		server.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	report, err := adapter.DetectCapabilities(context.Background())
	if err != nil || report.Features[CapabilityModelListing] != CapabilityUnsupported ||
		report.Features[CapabilityStreaming] != CapabilityUnknown {
		t.Fatalf("capabilities=%#v err=%v", report, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = adapter.Complete(ctx, modelRequest())
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("cancel error = %v", err)
	}
	timeoutServer := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		time.Sleep(50 * time.Millisecond)
	}))
	adapter, err = NewOpenAICompat(
		RoleConfig{
			Provider: ProviderOpenAICompatible,
			BaseURL:  timeoutServer.URL,
			Model:    "fixture",
			Timeout:  10 * time.Millisecond,
		},
		nil,
		timeoutServer.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	_, err = adapter.Complete(context.Background(), modelRequest())
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("timeout error = %v", err)
	}
}

func TestAnthropicNonStreamingStructuredToolAndProvenance(t *testing.T) {
	t.Parallel()
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.Header.Get("x-api-key") != "secret-token" ||
			request.Header.Get("anthropic-version") != "2023-06-01" {
			t.Fatalf("headers = %q/%q", request.Header.Get("x-api-key"), request.Header.Get("anthropic-version"))
		}
		var payload map[string]any
		if err := json.NewDecoder(request.Body).Decode(&payload); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if payload["max_tokens"] != float64(defaultAnthropicMaxTokens) || payload["system"] != "system" {
			t.Fatalf("payload = %#v", payload)
		}
		choice, ok := payload["tool_choice"].(map[string]any)
		if !ok || choice["name"] != structuredToolName {
			t.Fatalf("tool choice = %#v", payload["tool_choice"])
		}
		response.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(
			response,
			`{"type":"message","content":[{"type":"tool_use","name":"mire_structured_output","input":{"ok":true}}],"stop_reason":"tool_use","usage":{"input_tokens":2,"output_tokens":3}}`,
		)
	}))
	defer server.Close()
	adapter, err := NewAnthropic(
		RoleConfig{
			Provider:      ProviderAnthropic,
			BaseURL:       server.URL,
			Model:         "claude-fixture",
			CredentialRef: "test",
			Timeout:       time.Second,
		},
		testCredentials("secret-token"),
		server.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	result, err := adapter.Complete(context.Background(), modelRequest())
	if err != nil || string(result.Output) != `{"ok":true}` || result.Usage.TotalTokens != 5 ||
		result.FinishReason != "tool_use" {
		t.Fatalf("result=%#v err=%v", result, err)
	}
	metadata := adapter.Metadata()
	if metadata.Adapter != string(ProviderAnthropic) || metadata.Protocol != anthropicProtocolVersion {
		t.Fatalf("metadata = %#v", metadata)
	}
	report, err := adapter.DetectCapabilities(context.Background())
	if err != nil || report.Features[CapabilityStructured] != CapabilitySupported {
		t.Fatalf("capabilities=%#v err=%v", report, err)
	}
}

func TestAnthropicStreamingStructuredToolAndMalformedFrame(t *testing.T) {
	t.Parallel()
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Type", "text/event-stream")
		_, _ = io.WriteString(
			response,
			"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":2}}}\n\n",
		)
		_, _ = io.WriteString(
			response,
			"event: content_block_start\ndata: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"tool_use\",\"name\":\"mire_structured_output\"}}\n\n",
		)
		_, _ = io.WriteString(response, `event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{\"ok\":"}}

`)
		_, _ = io.WriteString(response, `event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"true}"}}

`)
		_, _ = io.WriteString(response, "event: content_block_stop\ndata: {\"type\":\"content_block_stop\"}\n\n")
		_, _ = io.WriteString(
			response,
			"event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
		)
		_, _ = io.WriteString(response, "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n")
	}))
	defer server.Close()
	adapter, err := NewAnthropic(
		RoleConfig{
			Provider: ProviderAnthropic,
			BaseURL:  server.URL,
			Model:    "fixture",
			Stream:   true,
			Timeout:  time.Second,
		},
		nil,
		server.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	result, err := adapter.Complete(context.Background(), modelRequest())
	if err != nil || string(result.Output) != `{"ok":true}` || result.Usage.TotalTokens != 5 ||
		result.FinishReason != "completed" {
		t.Fatalf("result=%#v err=%v", result, err)
	}

	badServer := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Type", "text/event-stream")
		_, _ = io.WriteString(response, "event: message_start\ndata: {\"type\":\"message_start\"}\n\n")
		_, _ = io.WriteString(response, "event: unknown\ndata: {\"type\":\"unknown\"}\n\n")
	}))
	defer badServer.Close()
	adapter, err = NewAnthropic(
		RoleConfig{
			Provider: ProviderAnthropic,
			BaseURL:  badServer.URL,
			Model:    "fixture",
			Stream:   true,
			Timeout:  time.Second,
		},
		nil,
		badServer.Client(),
	)
	if err != nil {
		t.Fatal(err)
	}
	_, err = adapter.Complete(context.Background(), modelRequest())
	var streamErr *MalformedStreamError
	if !errors.As(err, &streamErr) {
		t.Fatalf("malformed stream error = %v", err)
	}
}

func TestRouterRoleAliasesAndCredentialReferences(t *testing.T) {
	server := newTestServer(t, http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/models" {
			_, _ = io.WriteString(response, `{}`)
			return
		}
		response.WriteHeader(http.StatusNotFound)
	}))
	t.Setenv("MIRE_TEST_MODEL_KEY", "secret-token")
	shared := RoleConfig{
		Provider:      ProviderOpenAICompatible,
		BaseURL:       server.URL,
		Model:         "shared",
		CredentialRef: "env:MIRE_TEST_MODEL_KEY",
	}
	router, err := NewRouter(Config{Shared: &shared, Aliases: map[string]RoleConfig{
		"claude": {
			Provider:      ProviderAnthropic,
			BaseURL:       server.URL,
			Model:         "claude",
			CredentialRef: "env:MIRE_TEST_MODEL_KEY",
		},
	}, Roles: map[review.ModelRole]string{review.ModelRoleChat: "claude"}})
	if err != nil {
		t.Fatal(err)
	}
	plannerConfig, err := router.RoleConfig(review.ModelRolePlanner)
	if err != nil || plannerConfig.Model != "shared" || plannerConfig.CredentialRef != "env:MIRE_TEST_MODEL_KEY" {
		t.Fatalf("planner config=%#v err=%v", plannerConfig, err)
	}
	chatModel, err := router.Model(review.ModelRoleChat)
	if err != nil {
		t.Fatal(err)
	}
	if metadata, ok := chatModel.(review.ModelMetadataProvider); !ok ||
		metadata.Metadata().Adapter != string(ProviderAnthropic) {
		t.Fatalf("chat model metadata = %#v", chatModel)
	}
	for _, role := range []review.ModelRole{review.ModelRolePlanner, review.ModelRoleReviewer, review.ModelRoleVerifier, review.ModelRoleChat} {
		roleModel, modelErr := router.Model(role)
		if modelErr != nil {
			t.Fatalf("role %q model error = %v", role, modelErr)
		}
		if _, ok := roleModel.(review.ModelMetadataProvider); !ok {
			t.Fatalf("role %q does not expose metadata", role)
		}
	}
	if _, err := router.Model(review.ModelRole("bad")); err == nil {
		t.Fatal("unsupported role unexpectedly succeeded")
	}
	capabilities, err := router.Capabilities(context.Background(), review.ModelRolePlanner)
	if err != nil || capabilities.Features[CapabilityModelListing] != CapabilitySupported {
		t.Fatalf("router capabilities=%#v err=%v", capabilities, err)
	}
	if _, err := NewRouter(
		Config{Roles: map[review.ModelRole]string{review.ModelRoleChat: "missing"}, Aliases: map[string]RoleConfig{}},
	); err == nil {
		t.Fatal("missing alias unexpectedly succeeded")
	}
}

func TestAdaptersShareNormalizedDependencies(t *testing.T) {
	t.Parallel()

	client := &http.Client{}
	credentials := testCredentials("secret-token")
	tests := []struct {
		name     string
		provider Provider
		create   func(RoleConfig, CredentialResolver, *http.Client) (review.Model, error)
	}{
		{
			name:     "OpenAI-compatible",
			provider: ProviderOpenAICompatible,
			create: func(config RoleConfig, credentials CredentialResolver, client *http.Client) (review.Model, error) {
				return NewOpenAICompat(config, credentials, client)
			},
		},
		{
			name:     "Anthropic",
			provider: ProviderAnthropic,
			create: func(config RoleConfig, credentials CredentialResolver, client *http.Client) (review.Model, error) {
				return NewAnthropic(config, credentials, client)
			},
		},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			adapter, err := test.create(
				RoleConfig{Provider: test.provider, BaseURL: "https://models.example/", Model: "fixture"},
				credentials,
				client,
			)
			if err != nil {
				t.Fatalf("create adapter error = %v", err)
			}
			base := func() adapterBase {
				switch value := adapter.(type) {
				case *OpenAICompatible:
					return value.adapterBase
				case *Anthropic:
					return value.adapterBase
				default:
					t.Fatalf("unexpected adapter type %T", adapter)
					return adapterBase{}
				}
			}()
			if base.config.BaseURL != "https://models.example" || base.config.Model != "fixture" ||
				base.config.Timeout <= 0 || base.config.Retry.MaxAttempts != 2 ||
				base.credentials == nil || base.client != client {
				t.Fatalf("shared adapter dependencies = %#v", base)
			}
		})
	}
}

func TestEnvironmentCredentialResolverValidation(t *testing.T) {
	t.Parallel()
	resolver := EnvironmentCredentialResolver{}
	if _, err := resolver.Resolve(context.Background(), "raw-secret"); err == nil {
		t.Fatal("raw credential reference unexpectedly succeeded")
	}
	if _, err := resolver.Resolve(context.Background(), "env:MISSING_MODEL_KEY"); err == nil {
		t.Fatal("missing credential unexpectedly succeeded")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := resolver.Resolve(ctx, "env:MISSING_MODEL_KEY"); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled resolver error = %v", err)
	}
}

func TestProviderConfigurationRejectsUnsafeOrUnboundedValues(t *testing.T) {
	t.Parallel()
	cases := []RoleConfig{
		{Provider: ProviderOpenAICompatible, BaseURL: "http://user:pass@example.test", Model: "fixture"},
		{Provider: ProviderAnthropic, BaseURL: "https://example.test/?key=secret", Model: "fixture"},
		{
			Provider: ProviderAnthropic,
			BaseURL:  "https://example.test",
			Model:    "fixture",
			Retry:    RetryPolicy{MaxAttempts: 9},
		},
		{
			Provider: ProviderAnthropic,
			BaseURL:  "https://example.test",
			Model:    "fixture",
			Budget:   Budget{MaxOutputBytes: -1},
		},
	}
	for index, config := range cases {
		if _, err := NewModel(config, nil, nil); err == nil {
			t.Fatalf("case %d unexpectedly succeeded", index)
		}
	}
}

func modelRequest() review.ModelRequest {
	return review.ModelRequest{Role: review.ModelRoleReviewer, Messages: []review.Message{
		{Role: review.MessageRoleSystem, Content: "system"},
		{Role: review.MessageRoleUser, Content: "review this"},
	}, Tools: []review.ToolDefinition{{Name: "snapshot_read", Description: "read", InputSchema: `{"type":"object"}`}}, Output: review.StructuredOutput{Schema: "mire/v1/result"}}
}

func newTestServer(t *testing.T, handler http.Handler) *httptest.Server {
	t.Helper()
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen for fixture server: %v", err)
	}
	server := httptest.NewUnstartedServer(handler)
	server.Listener = listener
	server.Start()
	t.Cleanup(server.Close)
	return server
}

func writeSSEPayload(writer io.Writer, payload any) {
	data, _ := json.Marshal(payload)
	_, _ = fmt.Fprintf(writer, "data: %s\n\n", data)
}

func testCredentials(value string) CredentialResolver {
	return CredentialResolverFunc(func(ctx context.Context, reference string) (string, error) {
		if err := ctx.Err(); err != nil {
			return "", err
		}
		if reference != "" && reference != "test" {
			return "", fmt.Errorf("unknown credential reference %q", reference)
		}
		return value, nil
	})
}
