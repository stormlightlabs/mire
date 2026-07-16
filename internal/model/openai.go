package model

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

const openAIProtocolVersion = "openai-chat-completions/v1"

// OpenAICompatible is an adapter for OpenAI Chat Completions-compatible
// endpoints. It deliberately uses local wire structs so compatible services
// are not coupled to the official OpenAI SDK's types.
type OpenAICompatible struct {
	adapterBase
}

// NewOpenAICompat creates an OpenAI-compatible adapter without resolving the configured credential.
func NewOpenAICompat(
	config RoleConfig,
	credentials CredentialResolver,
	client *http.Client,
) (*OpenAICompatible, error) {
	base, err := newAdapterBase(config, credentials, client)
	if err != nil {
		return nil, err
	}
	if base.config.Provider != ProviderOpenAICompatible {
		return nil, fmt.Errorf("create OpenAI-compatible model: provider %q is unsupported", base.config.Provider)
	}
	if _, err := endpoint(base.config.BaseURL, "chat/completions"); err != nil {
		return nil, fmt.Errorf("create OpenAI-compatible model: %w", err)
	}
	return &OpenAICompatible{adapterBase: base}, nil
}

// Metadata returns credential-free provenance for durable review runs.
func (adapter *OpenAICompatible) Metadata() review.ModelMetadata {
	if adapter == nil {
		return review.ModelMetadata{}
	}
	return adapter.adapterBase.metadata(openAIProtocolVersion)
}

// Complete translates a provider-neutral request to Chat Completions and
// translates either a JSON response or SSE stream back to ModelResponse.
func (adapter *OpenAICompatible) Complete(
	ctx context.Context,
	request review.ModelRequest,
) (review.ModelResponse, error) {
	if adapter == nil {
		return review.ModelResponse{}, errors.New("complete OpenAI-compatible model: adapter is nil")
	}
	if err := contextError(ctx); err != nil {
		return review.ModelResponse{}, err
	}
	modelName := strings.TrimSpace(request.Model)
	if modelName == "" {
		modelName = adapter.config.Model
	}
	messages, err := openAIMessages(request)
	if err != nil {
		return review.ModelResponse{}, fmt.Errorf("complete OpenAI-compatible model: %w", err)
	}
	payload := make(map[string]any, len(request.Parameters)+6)
	for key, value := range request.Parameters {
		payload[key] = value
	}
	payload["model"] = modelName
	payload["messages"] = messages
	payload["stream"] = adapter.config.Stream
	if adapter.config.Stream {
		payload["stream_options"] = map[string]any{"include_usage": true}
	}
	if request.Output.Schema != "" {
		payload["response_format"] = map[string]any{"type": "json_object"}
	}
	if adapter.config.Budget.MaxOutputTokens > 0 {
		if _, hasMaxTokens := payload["max_tokens"]; !hasMaxTokens {
			if _, hasCompletionTokens := payload["max_completion_tokens"]; !hasCompletionTokens {
				payload["max_tokens"] = adapter.config.Budget.MaxOutputTokens
			}
		}
	}
	tools := make([]any, 0, len(request.Tools))
	for index, tool := range request.Tools {
		converted, convertErr := convertOpenAITool(tool)
		if convertErr != nil {
			return review.ModelResponse{}, fmt.Errorf(
				"complete OpenAI-compatible model: tool %d: %w",
				index,
				convertErr,
			)
		}
		tools = append(tools, converted)
	}
	if len(tools) > 0 {
		payload["tools"] = tools
	}
	if request.Repair && strings.TrimSpace(request.PreviousOutput) != "" {
		payload["messages"] = append(
			messages,
			openAIMessage{
				Role:    "user",
				Content: "The previous structured response was invalid. Repair it and return only valid JSON. Previous response:\n" + request.PreviousOutput,
			},
		)
	}
	body, err := json.Marshal(payload)
	if err != nil {
		return review.ModelResponse{}, fmt.Errorf("encode OpenAI-compatible request: %w", err)
	}
	credential, err := resolveCredential(ctx, adapter.credentials, adapter.config.CredentialRef)
	if err != nil {
		return review.ModelResponse{}, err
	}
	requestURL, err := endpoint(adapter.config.BaseURL, "chat/completions")
	if err != nil {
		return review.ModelResponse{}, err
	}
	headers := make(http.Header)
	headers.Set("User-Agent", "mire/1")
	if credential != "" {
		headers.Set("Authorization", "Bearer "+credential)
	}
	return execute(
		ctx,
		adapter.client,
		ProviderOpenAICompatible,
		"chat completions",
		requestURL,
		string(body),
		credential,
		headers,
		adapter.config,
		func(reader io.Reader) (review.ModelResponse, error) {
			if adapter.config.Stream {
				return parseOpenAIStream(reader, adapter.config.Budget)
			}
			return parseOpenAIResponse(reader, adapter.config.Budget)
		},
	)
}

// DetectCapabilities reports guaranteed protocol support and leaves optional
// OpenAI-compatible features unknown unless the endpoint explicitly overrides
// them in RoleConfig.Capabilities. The models endpoint is probed only to
// distinguish model-listing support; it cannot prove feature support.
func (adapter *OpenAICompatible) DetectCapabilities(ctx context.Context) (CapabilityReport, error) {
	if adapter == nil {
		return CapabilityReport{}, errors.New("detect OpenAI-compatible capabilities: adapter is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	report := CapabilityReport{
		Provider: ProviderOpenAICompatible, BaseURL: adapter.config.BaseURL, Model: adapter.config.Model,
		Features: map[Capability]CapabilityStatus{
			CapabilityChatCompletions: CapabilitySupported,
			CapabilityStreaming:       CapabilityUnknown,
			CapabilityStructured:      CapabilityUnknown,
			CapabilityToolUse:         CapabilityUnknown,
			CapabilityUsage:           CapabilityUnknown,
			CapabilityModelListing:    CapabilityUnknown,
		}, CheckedAt: time.Now().UTC(),
	}
	for capability, status := range adapter.config.Capabilities {
		report.Features[capability] = status
	}
	credential, err := resolveCredential(ctx, adapter.credentials, adapter.config.CredentialRef)
	if err != nil {
		return report, err
	}
	probeCtx, cancel := context.WithTimeout(ctx, adapter.config.Timeout)
	defer cancel()
	requestURL, err := endpoint(adapter.config.BaseURL, "models")
	if err != nil {
		return report, err
	}
	request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, requestURL, nil)
	if err != nil {
		return report, fmt.Errorf("create capability request: %w", err)
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set("User-Agent", "mire/1")
	if credential != "" {
		request.Header.Set("Authorization", "Bearer "+credential)
	}
	response, err := adapter.client.Do(request)
	if err != nil {
		if probeCtx.Err() != nil {
			return report, probeCtx.Err()
		}
		return report, fmt.Errorf("probe OpenAI-compatible capabilities: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		providerErr := decodeProviderError(
			ProviderOpenAICompatible,
			"capability probe",
			response.StatusCode,
			response.Header,
			response.Body,
			credential,
		)
		if response.StatusCode == http.StatusNotFound || response.StatusCode == http.StatusMethodNotAllowed {
			report.Features[CapabilityModelListing] = CapabilityUnsupported
			report.Limitations = append(
				report.Limitations,
				"The endpoint does not expose the optional /models capability probe.",
			)
			return report, nil
		}
		return report, providerErr
	}
	_, readErr := io.Copy(io.Discard, io.LimitReader(response.Body, maxProviderErrorBytes))
	if readErr != nil {
		return report, fmt.Errorf("read capability response: %w", readErr)
	}
	report.Features[CapabilityModelListing] = CapabilitySupported
	report.Limitations = append(
		report.Limitations,
		"Streaming, tools, and structured output remain endpoint/model-specific and were not inferred from /models.",
	)
	return report, nil
}

type openAIMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type openAIToolPayload struct {
	Type     string         `json:"type"`
	Function openAIFunction `json:"function"`
}

type openAIFunction struct {
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	Parameters  json.RawMessage `json:"parameters"`
}

func openAIMessages(request review.ModelRequest) ([]openAIMessage, error) {
	result := make([]openAIMessage, 0, len(request.Messages)+1)
	for index, message := range request.Messages {
		role := string(message.Role)
		switch message.Role {
		case review.MessageRoleSystem, review.MessageRoleUser, review.MessageRoleAssistant, review.MessageRoleTool:
		default:
			return nil, fmt.Errorf("message %d has unsupported role %q", index, message.Role)
		}
		result = append(result, openAIMessage{Role: role, Content: message.Content})
	}
	return result, nil
}

func convertOpenAITool(tool review.ToolDefinition) (openAIToolPayload, error) {
	if strings.TrimSpace(tool.Name) == "" {
		return openAIToolPayload{}, errors.New("tool name is required")
	}
	schema := strings.TrimSpace(tool.InputSchema)
	if schema == "" {
		schema = `{"type":"object"}`
	}
	var raw json.RawMessage
	if !json.Valid([]byte(schema)) {
		return openAIToolPayload{}, errors.New("tool input schema is not valid JSON")
	}
	raw = json.RawMessage(schema)
	return openAIToolPayload{
		Type:     "function",
		Function: openAIFunction{Name: tool.Name, Description: tool.Description, Parameters: raw},
	}, nil
}

func parseOpenAIResponse(reader io.Reader, budget Budget) (review.ModelResponse, error) {
	decoder := json.NewDecoder(io.LimitReader(reader, 8<<20))
	var envelope struct {
		Error   json.RawMessage `json:"error"`
		Choices []struct {
			Message struct {
				Content   json.RawMessage `json:"content"`
				ToolCalls []struct {
					Function struct {
						Arguments string `json:"arguments"`
					} `json:"function"`
				} `json:"tool_calls"`
			} `json:"message"`
			FinishReason string `json:"finish_reason"`
		} `json:"choices"`
		Usage openAIUsage `json:"usage"`
	}
	if err := decoder.Decode(&envelope); err != nil {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderOpenAICompatible,
			Operation: "chat completions",
			Reason:    "invalid JSON",
		}
	}
	if err := requireJSONEOF(decoder); err != nil {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderOpenAICompatible,
			Operation: "chat completions",
			Reason:    "trailing JSON",
		}
	}
	if len(envelope.Error) > 0 && string(envelope.Error) != "null" {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderOpenAICompatible,
			Operation: "chat completions",
			Reason:    "provider returned an error payload with a success status",
		}
	}
	if len(envelope.Choices) == 0 {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderOpenAICompatible,
			Operation: "chat completions",
			Reason:    "response contains no choices",
		}
	}
	choice := envelope.Choices[0]
	text, err := contentText(choice.Message.Content)
	if err != nil {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderOpenAICompatible,
			Operation: "chat completions",
			Reason:    "message content has an unsupported shape",
		}
	}
	if text == "" && len(choice.Message.ToolCalls) > 0 {
		text = choice.Message.ToolCalls[0].Function.Arguments
	}
	if budget.MaxOutputBytes > 0 && len(text) > budget.MaxOutputBytes {
		return review.ModelResponse{}, &BudgetError{
			Kind:  "output bytes",
			Value: int64(len(text)),
			Limit: int64(budget.MaxOutputBytes),
		}
	}
	return review.ModelResponse{
		Output:       []byte(text),
		Usage:        envelope.Usage.normalize(),
		FinishReason: normalizeFinishReason(choice.FinishReason),
	}, nil
}

func parseOpenAIStream(reader io.Reader, budget Budget) (review.ModelResponse, error) {
	var output strings.Builder
	var toolArguments strings.Builder
	var usage review.Usage
	finishReason := ""
	seenDone := false
	if err := readSSE(reader, ProviderOpenAICompatible, func(eventNumber int, event sseEvent) error {
		data := strings.TrimSpace(event.Data)
		if data == "[DONE]" {
			seenDone = true
			return nil
		}
		var chunk struct {
			Error   json.RawMessage `json:"error"`
			Choices []struct {
				Delta struct {
					Content   json.RawMessage `json:"content"`
					ToolCalls []struct {
						Function struct {
							Arguments string `json:"arguments"`
						} `json:"function"`
					} `json:"tool_calls"`
				} `json:"delta"`
				FinishReason *string `json:"finish_reason"`
			} `json:"choices"`
			Usage *openAIUsage `json:"usage"`
		}
		if err := json.Unmarshal([]byte(data), &chunk); err != nil {
			return &MalformedStreamError{
				Provider: ProviderOpenAICompatible,
				Event:    eventNumber,
				Reason:   "event data is not valid JSON",
			}
		}
		if len(chunk.Error) > 0 && string(chunk.Error) != "null" {
			return &ProviderError{
				Provider:   ProviderOpenAICompatible,
				Operation:  "chat completions stream",
				StatusCode: http.StatusOK,
				Message:    "provider returned a stream error",
				Retryable:  false,
			}
		}
		if chunk.Usage != nil {
			usage = chunk.Usage.normalize()
		}
		if len(chunk.Choices) == 0 {
			if chunk.Usage == nil {
				return &MalformedStreamError{
					Provider: ProviderOpenAICompatible,
					Event:    eventNumber,
					Reason:   "event contains neither a choice nor usage",
				}
			}
			return nil
		}
		choice := chunk.Choices[0]
		text, err := contentText(choice.Delta.Content)
		if err != nil {
			return &MalformedStreamError{
				Provider: ProviderOpenAICompatible,
				Event:    eventNumber,
				Reason:   "content delta has an unsupported shape",
			}
		}
		output.WriteString(text)
		for _, toolCall := range choice.Delta.ToolCalls {
			toolArguments.WriteString(toolCall.Function.Arguments)
		}
		if choice.FinishReason != nil {
			finishReason = *choice.FinishReason
		}
		if budget.MaxOutputBytes > 0 && output.Len()+toolArguments.Len() > budget.MaxOutputBytes {
			return &BudgetError{
				Kind:  "output bytes",
				Value: int64(output.Len() + toolArguments.Len()),
				Limit: int64(budget.MaxOutputBytes),
			}
		}
		return nil
	}); err != nil {
		return review.ModelResponse{}, err
	}
	if !seenDone {
		return review.ModelResponse{}, &MalformedStreamError{
			Provider: ProviderOpenAICompatible,
			Event:    0,
			Reason:   "stream ended before [DONE]",
		}
	}
	result := output.String()
	if result == "" {
		result = toolArguments.String()
	}
	return review.ModelResponse{
		Output:       []byte(result),
		Usage:        usage,
		FinishReason: normalizeFinishReason(finishReason),
	}, nil
}

type openAIUsage struct {
	PromptTokens     int64 `json:"prompt_tokens"`
	CompletionTokens int64 `json:"completion_tokens"`
	TotalTokens      int64 `json:"total_tokens"`
}

func (usage openAIUsage) normalize() review.Usage {
	total := usage.TotalTokens
	if total == 0 {
		total = usage.PromptTokens + usage.CompletionTokens
	}
	return review.Usage{InputTokens: usage.PromptTokens, OutputTokens: usage.CompletionTokens, TotalTokens: total}
}
