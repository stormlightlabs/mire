package model

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestHTTPHelpersNormalizeReasonsContentAndBudgets(t *testing.T) {
	t.Parallel()
	for _, test := range []struct {
		reason string
		want   string
	}{
		{reason: "", want: "completed"}, {reason: "stop", want: "completed"},
		{reason: "end_turn", want: "completed"}, {reason: "length", want: "max_tokens"},
		{reason: "max_tokens", want: "max_tokens"}, {reason: "tool_calls", want: "tool_use"},
		{reason: "function_call", want: "tool_use"}, {reason: "tool_use", want: "tool_use"},
		{reason: "content_filter", want: "refused"}, {reason: "refusal", want: "refused"},
		{reason: "stop_sequence", want: "stop_sequence"}, {reason: "pause_turn", want: "paused"},
		{reason: "provider-specific", want: "provider-specific"},
	} {
		if got := normalizeFinishReason(test.reason); got != test.want {
			t.Errorf("normalizeFinishReason(%q) = %q, want %q", test.reason, got, test.want)
		}
	}
	text, err := contentText([]byte(`[{"type":"text","text":"a"},{"type":"output_text","text":"b"}]`))
	if err != nil || text != "ab" {
		t.Fatalf("contentText blocks = %q, %v", text, err)
	}
	text, err = contentText([]byte(`"plain"`))
	if err != nil || text != "plain" {
		t.Fatalf("contentText string = %q, %v", text, err)
	}
	if _, err := contentText([]byte(`{"bad":`)); err == nil {
		t.Fatal("invalid content unexpectedly succeeded")
	}
	if text, err := contentText(nil); err != nil || text != "" {
		t.Fatalf("empty content = %q, %v", text, err)
	}
	if err := enforceUsageBudget(review.Usage{InputTokens: 4}, Budget{MaxInputTokens: 3}); err == nil {
		t.Fatal("input budget unexpectedly succeeded")
	}
	if err := enforceUsageBudget(review.Usage{OutputTokens: 4}, Budget{MaxOutputTokens: 3}); err == nil {
		t.Fatal("output budget unexpectedly succeeded")
	}
	if err := enforceUsageBudget(review.Usage{InputTokens: 2, OutputTokens: 2}, Budget{MaxTotalTokens: 3}); err == nil {
		t.Fatal("total budget unexpectedly succeeded")
	}
	if err := enforceUsageBudget(review.Usage{InputTokens: 2, OutputTokens: 2}, Budget{MaxTotalTokens: 4}); err != nil {
		t.Fatalf("exact total budget error = %v", err)
	}
	decoder := jsonDecoder(`{"ok":true}`)
	var value any
	if err := decoder.Decode(&value); err != nil {
		t.Fatalf("decode valid JSON = %v", err)
	}
	if err := requireJSONEOF(decoder); err != nil {
		t.Fatalf("valid JSON EOF = %v", err)
	}
	decoder = jsonDecoder(`{"ok":true}{"extra":true}`)
	if err := decoder.Decode(&value); err != nil {
		t.Fatalf("decode first JSON = %v", err)
	}
	if err := requireJSONEOF(decoder); err == nil {
		t.Fatal("trailing JSON unexpectedly succeeded")
	}
}

func TestHTTPHelpersRedactErrorsRetryAfterAndEndpoints(t *testing.T) {
	t.Parallel()
	if got := sanitizeText("Authorization: secret, Bearer sk-ant-abc", "secret"); strings.Contains(got, "secret") || strings.Contains(got, "sk-ant-abc") {
		t.Fatalf("sanitized text = %q", got)
	}
	if got := sanitizeError(errors.New("secret-token"), "secret-token").Error(); got != "[REDACTED]" {
		t.Fatalf("sanitized error = %q", got)
	}
	if got := (&ProviderError{Provider: ProviderAnthropic, Operation: "messages", StatusCode: http.StatusTooManyRequests, Code: "rate_limit", Message: "retry"}).Error(); !strings.Contains(got, "status 429") {
		t.Fatalf("provider error = %q", got)
	}
	if (&BudgetError{Kind: "output bytes", Value: 4, Limit: 3}).Error() == "" || (&MalformedResponseError{Provider: ProviderAnthropic, Operation: "messages", Reason: "bad"}).Error() == "" || (&MalformedStreamError{Provider: ProviderAnthropic, Event: 1, Reason: "bad"}).Error() == "" {
		t.Fatal("error types did not describe themselves")
	}
	if (&BudgetError{}).Error() == "" || (&MalformedResponseError{}).Error() == "" || (&MalformedStreamError{}).Error() == "" {
		t.Fatal("zero error types did not describe themselves")
	}
	if parseRetryAfter(http.Header{"Retry-After": []string{"3"}}) != 3*time.Second {
		t.Fatalf("retry-after seconds = %v", parseRetryAfter(http.Header{"Retry-After": []string{"3"}}))
	}
	if parseRetryAfter(http.Header{"Retry-After": []string{"invalid"}}) != 0 {
		t.Fatal("invalid retry-after unexpectedly parsed")
	}
	if _, err := endpoint("https://example.test/v1", "messages"); err != nil {
		t.Fatalf("valid endpoint error = %v", err)
	}
	for _, baseURL := range []string{"", "example.test", "ftp://example.test", "https://user:pass@example.test", "https://example.test?q=secret", "https://example.test#fragment"} {
		if _, err := endpoint(baseURL, "messages"); err == nil {
			t.Errorf("endpoint(%q) unexpectedly succeeded", baseURL)
		}
	}
	if retryableStatus(http.StatusTooManyRequests) != true || retryableStatus(http.StatusBadRequest) != false {
		t.Fatal("retryable status classification is wrong")
	}
}

func TestRetryWaitAndSSEParsingRespectCancellation(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := waitRetry(ctx, RetryPolicy{InitialDelay: time.Hour, MaxDelay: time.Hour}, 1, 0); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled retry error = %v", err)
	}
	var events []sseEvent
	if err := readSSE(strings.NewReader(": keepalive\nevent: message\ndata: one\ndata: two\n\n"), ProviderAnthropic, func(_ int, event sseEvent) error {
		events = append(events, event)
		return nil
	}); err != nil || len(events) != 1 || events[0].Name != "message" || events[0].Data != "one\ntwo" {
		t.Fatalf("SSE events=%#v err=%v", events, err)
	}
	if err := readSSE(strings.NewReader("unknown: field\n\n"), ProviderAnthropic, func(_ int, _ sseEvent) error { return nil }); err == nil {
		t.Fatal("unknown SSE field unexpectedly succeeded")
	}
}

func jsonDecoder(value string) *json.Decoder {
	return json.NewDecoder(strings.NewReader(value))
}
