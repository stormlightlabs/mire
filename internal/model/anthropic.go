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

const (
	anthropicProtocolVersion  = "anthropic-messages/v1"
	structuredToolName        = "mire_structured_output"
	defaultAnthropicMaxTokens = 4096
)

// Anthropic is an adapter for Anthropic's native Messages API.
type Anthropic struct {
	adapterBase
}

// NewAnthropic creates an Anthropic adapter without resolving the configured
// credential.
func NewAnthropic(config RoleConfig, credentials CredentialResolver, client *http.Client) (*Anthropic, error) {
	base, err := newAdapterBase(config, credentials, client)
	if err != nil {
		return nil, err
	}
	if base.config.Provider != ProviderAnthropic {
		return nil, fmt.Errorf("create Anthropic model: provider %q is unsupported", base.config.Provider)
	}
	if _, err := endpoint(base.config.BaseURL, "messages"); err != nil {
		return nil, fmt.Errorf("create Anthropic model: %w", err)
	}
	return &Anthropic{adapterBase: base}, nil
}

// Metadata returns credential-free provenance for durable review runs.
func (adapter *Anthropic) Metadata() review.ModelMetadata {
	if adapter == nil {
		return review.ModelMetadata{}
	}
	return adapter.adapterBase.metadata(anthropicProtocolVersion)
}

// Complete translates a provider-neutral request to Anthropic Messages and
// supports both complete responses and native SSE events. Structured outputs
// use a forced tool so tool JSON can share the domain's strict validation path.
func (adapter *Anthropic) Complete(ctx context.Context, request review.ModelRequest) (review.ModelResponse, error) {
	if adapter == nil {
		return review.ModelResponse{}, errors.New("complete Anthropic model: adapter is nil")
	}
	if err := contextError(ctx); err != nil {
		return review.ModelResponse{}, err
	}
	modelName := strings.TrimSpace(request.Model)
	if modelName == "" {
		modelName = adapter.config.Model
	}
	system, messages, err := anthropicMessages(request)
	if err != nil {
		return review.ModelResponse{}, fmt.Errorf("complete Anthropic model: %w", err)
	}
	if request.Repair && strings.TrimSpace(request.PreviousOutput) != "" {
		messages = append(
			messages,
			anthropicMessage{
				Role:    "user",
				Content: "The previous structured response was invalid. Repair it and return only the required result. Previous response:\n" + request.PreviousOutput,
			},
		)
	}
	payload := make(map[string]any, len(request.Parameters)+8)
	for key, value := range request.Parameters {
		payload[key] = value
	}
	payload["model"] = modelName
	payload["messages"] = messages
	if system != "" {
		payload["system"] = system
	}
	maxTokens := int64(defaultAnthropicMaxTokens)
	if adapter.config.Budget.MaxOutputTokens > 0 {
		maxTokens = adapter.config.Budget.MaxOutputTokens
	}
	if configured, ok := payload["max_tokens"].(int); ok && configured > 0 {
		maxTokens = int64(configured)
	}
	payload["max_tokens"] = maxTokens
	payload["stream"] = adapter.config.Stream
	tools := make([]anthropicTool, 0, len(request.Tools)+1)
	for index, tool := range request.Tools {
		converted, convertErr := anthropicToolFromDefinition(tool)
		if convertErr != nil {
			return review.ModelResponse{}, fmt.Errorf("complete Anthropic model: tool %d: %w", index, convertErr)
		}
		tools = append(tools, converted)
	}
	if request.Output.Schema != "" {
		tools = append(
			tools,
			anthropicTool{
				Name:        structuredToolName,
				Description: "Return the required provider-neutral structured response.",
				InputSchema: json.RawMessage(`{"type":"object","additionalProperties":true}`),
			},
		)
		payload["tool_choice"] = map[string]any{"type": "tool", "name": structuredToolName}
	}
	if len(tools) > 0 {
		payload["tools"] = tools
	}
	body, err := json.Marshal(payload)
	if err != nil {
		return review.ModelResponse{}, fmt.Errorf("encode Anthropic request: %w", err)
	}
	credential, err := resolveCredential(ctx, adapter.credentials, adapter.config.CredentialRef)
	if err != nil {
		return review.ModelResponse{}, err
	}
	requestURL, err := endpoint(adapter.config.BaseURL, "messages")
	if err != nil {
		return review.ModelResponse{}, err
	}
	headers := make(http.Header)
	headers.Set("User-Agent", "mire/1")
	headers.Set("anthropic-version", "2023-06-01")
	if credential != "" {
		headers.Set("x-api-key", credential)
	}
	return execute(
		ctx,
		adapter.client,
		ProviderAnthropic,
		"messages",
		requestURL,
		string(body),
		credential,
		headers,
		adapter.config,
		func(reader io.Reader) (review.ModelResponse, error) {
			if adapter.config.Stream {
				return parseAnthropicStream(reader, adapter.config.Budget)
			}
			return parseAnthropicResponse(reader, adapter.config.Budget)
		},
	)
}

// DetectCapabilities reports the native Messages features used by this
// adapter. Overrides may mark model-specific features as unknown or
// unsupported without changing the transport.
func (adapter *Anthropic) DetectCapabilities(ctx context.Context) (CapabilityReport, error) {
	if adapter == nil {
		return CapabilityReport{}, errors.New("detect Anthropic capabilities: adapter is nil")
	}
	if err := contextError(ctx); err != nil {
		return CapabilityReport{}, err
	}
	report := CapabilityReport{
		Provider: ProviderAnthropic,
		BaseURL:  adapter.config.BaseURL,
		Model:    adapter.config.Model,
		Features: map[Capability]CapabilityStatus{
			CapabilityMessages:     CapabilitySupported,
			CapabilityStreaming:    CapabilitySupported,
			CapabilityStructured:   CapabilitySupported,
			CapabilityToolUse:      CapabilitySupported,
			CapabilityUsage:        CapabilitySupported,
			CapabilityModelListing: CapabilityUnsupported,
		},
		CheckedAt: time.Now().UTC(),
		Limitations: []string{
			"Structured output is transported as a forced tool-use block and remains subject to domain schema validation.",
		},
	}
	for capability, status := range adapter.config.Capabilities {
		report.Features[capability] = status
	}
	return report, nil
}

type anthropicMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type anthropicTool struct {
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	InputSchema json.RawMessage `json:"input_schema"`
}

func anthropicMessages(request review.ModelRequest) (string, []anthropicMessage, error) {
	var systemParts []string
	messages := make([]anthropicMessage, 0, len(request.Messages))
	for index, message := range request.Messages {
		switch message.Role {
		case review.MessageRoleSystem:
			systemParts = append(systemParts, message.Content)
		case review.MessageRoleUser, review.MessageRoleAssistant:
			messages = append(messages, anthropicMessage{Role: string(message.Role), Content: message.Content})
		case review.MessageRoleTool:
			messages = append(messages, anthropicMessage{Role: "user", Content: "Tool context:\n" + message.Content})
		default:
			return "", nil, fmt.Errorf("message %d has unsupported role %q", index, message.Role)
		}
	}
	return strings.Join(systemParts, "\n\n"), messages, nil
}

func anthropicToolFromDefinition(tool review.ToolDefinition) (anthropicTool, error) {
	if strings.TrimSpace(tool.Name) == "" {
		return anthropicTool{}, errors.New("tool name is required")
	}
	schema := strings.TrimSpace(tool.InputSchema)
	if schema == "" {
		schema = `{"type":"object"}`
	}
	if !json.Valid([]byte(schema)) {
		return anthropicTool{}, errors.New("tool input schema is not valid JSON")
	}
	return anthropicTool{Name: tool.Name, Description: tool.Description, InputSchema: json.RawMessage(schema)}, nil
}

type anthropicContentBlock struct {
	Type  string          `json:"type"`
	Text  string          `json:"text,omitempty"`
	Name  string          `json:"name,omitempty"`
	Input json.RawMessage `json:"input,omitempty"`
}

type anthropicUsage struct {
	InputTokens  int64 `json:"input_tokens"`
	OutputTokens int64 `json:"output_tokens"`
}

func (usage anthropicUsage) normalize() review.Usage {
	return review.Usage{
		InputTokens:  usage.InputTokens,
		OutputTokens: usage.OutputTokens,
		TotalTokens:  usage.InputTokens + usage.OutputTokens,
	}
}

func parseAnthropicResponse(reader io.Reader, budget Budget) (review.ModelResponse, error) {
	decoder := json.NewDecoder(io.LimitReader(reader, 8<<20))
	var envelope struct {
		Type       string                  `json:"type"`
		Content    []anthropicContentBlock `json:"content"`
		StopReason string                  `json:"stop_reason"`
		Usage      anthropicUsage          `json:"usage"`
		Error      json.RawMessage         `json:"error"`
	}
	if err := decoder.Decode(&envelope); err != nil {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderAnthropic,
			Operation: "messages",
			Reason:    "invalid JSON",
		}
	}
	if err := requireJSONEOF(decoder); err != nil {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderAnthropic,
			Operation: "messages",
			Reason:    "trailing JSON",
		}
	}
	if envelope.Type != "message" {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderAnthropic,
			Operation: "messages",
			Reason:    "response type is not message",
		}
	}
	if len(envelope.Error) > 0 && string(envelope.Error) != "null" {
		return review.ModelResponse{}, &MalformedResponseError{
			Provider:  ProviderAnthropic,
			Operation: "messages",
			Reason:    "provider returned an error payload with a success status",
		}
	}
	output, err := anthropicOutput(envelope.Content, budget)
	if err != nil {
		return review.ModelResponse{}, err
	}
	return review.ModelResponse{
		Output:       []byte(output),
		Usage:        envelope.Usage.normalize(),
		FinishReason: normalizeFinishReason(envelope.StopReason),
	}, nil
}

func anthropicOutput(blocks []anthropicContentBlock, budget Budget) (string, error) {
	var text strings.Builder
	for _, block := range blocks {
		switch block.Type {
		case "text":
			text.WriteString(block.Text)
		case "tool_use":
			if block.Name != structuredToolName {
				continue
			}
			if len(block.Input) == 0 || string(block.Input) == "null" {
				return "", &MalformedResponseError{
					Provider:  ProviderAnthropic,
					Operation: "messages",
					Reason:    "structured tool-use block has no input",
				}
			}
			if budget.MaxOutputBytes > 0 && len(block.Input) > budget.MaxOutputBytes {
				return "", &BudgetError{
					Kind:  "output bytes",
					Value: int64(len(block.Input)),
					Limit: int64(budget.MaxOutputBytes),
				}
			}
			return string(block.Input), nil
		}
	}
	if budget.MaxOutputBytes > 0 && text.Len() > budget.MaxOutputBytes {
		return "", &BudgetError{Kind: "output bytes", Value: int64(text.Len()), Limit: int64(budget.MaxOutputBytes)}
	}
	return text.String(), nil
}

func parseAnthropicStream(reader io.Reader, budget Budget) (review.ModelResponse, error) {
	var text strings.Builder
	var toolInput strings.Builder
	var usage review.Usage
	finishReason := ""
	currentBlock := ""
	currentToolName := ""
	seenMessageStart := false
	seenMessageStop := false
	if err := readSSE(reader, ProviderAnthropic, func(eventNumber int, event sseEvent) error {
		var envelope struct {
			Type    string `json:"type"`
			Message struct {
				Usage anthropicUsage `json:"usage"`
			} `json:"message"`
			ContentBlock anthropicContentBlock `json:"content_block"`
			Delta        struct {
				Type        string `json:"type"`
				Text        string `json:"text"`
				PartialJSON string `json:"partial_json"`
				StopReason  string `json:"stop_reason"`
				Usage       struct {
					OutputTokens int64 `json:"output_tokens"`
				} `json:"usage"`
			} `json:"delta"`
			Error struct {
				Type    string `json:"type"`
				Message string `json:"message"`
			} `json:"error"`
			Usage struct {
				OutputTokens int64 `json:"output_tokens"`
			} `json:"usage"`
		}
		if err := json.Unmarshal([]byte(event.Data), &envelope); err != nil {
			return &MalformedStreamError{
				Provider: ProviderAnthropic,
				Event:    eventNumber,
				Reason:   "event data is not valid JSON",
			}
		}
		if envelope.Type == "error" {
			return &ProviderError{
				Provider:   ProviderAnthropic,
				Operation:  "messages stream",
				StatusCode: http.StatusOK,
				Message:    "provider returned a stream error",
			}
		}
		switch envelope.Type {
		case "message_start":
			seenMessageStart = true
			usage = envelope.Message.Usage.normalize()
		case "content_block_start":
			if !seenMessageStart {
				return &MalformedStreamError{
					Provider: ProviderAnthropic,
					Event:    eventNumber,
					Reason:   "content block started before message_start",
				}
			}
			currentBlock = envelope.ContentBlock.Type
			currentToolName = envelope.ContentBlock.Name
		case "content_block_delta":
			switch envelope.Delta.Type {
			case "text_delta":
				if currentBlock != "text" {
					return &MalformedStreamError{
						Provider: ProviderAnthropic,
						Event:    eventNumber,
						Reason:   "text delta does not belong to a text block",
					}
				}
				text.WriteString(envelope.Delta.Text)
			case "input_json_delta":
				if currentBlock != "tool_use" || currentToolName != structuredToolName {
					return &MalformedStreamError{
						Provider: ProviderAnthropic,
						Event:    eventNumber,
						Reason:   "JSON delta does not belong to the structured output tool",
					}
				}
				toolInput.WriteString(envelope.Delta.PartialJSON)
			default:
				return &MalformedStreamError{
					Provider: ProviderAnthropic,
					Event:    eventNumber,
					Reason:   "unsupported content delta",
				}
			}
		case "content_block_stop":
			currentBlock = ""
			currentToolName = ""
		case "message_delta":
			finishReason = envelope.Delta.StopReason
			if envelope.Usage.OutputTokens > 0 {
				usage.OutputTokens = envelope.Usage.OutputTokens
				usage.TotalTokens = usage.InputTokens + usage.OutputTokens
			}
		case "message_stop":
			seenMessageStop = true
		default:
			return &MalformedStreamError{
				Provider: ProviderAnthropic,
				Event:    eventNumber,
				Reason:   "unsupported event type",
			}
		}
		if budget.MaxOutputBytes > 0 && text.Len()+toolInput.Len() > budget.MaxOutputBytes {
			return &BudgetError{
				Kind:  "output bytes",
				Value: int64(text.Len() + toolInput.Len()),
				Limit: int64(budget.MaxOutputBytes),
			}
		}
		return nil
	}); err != nil {
		return review.ModelResponse{}, err
	}
	if !seenMessageStart || !seenMessageStop {
		return review.ModelResponse{}, &MalformedStreamError{
			Provider: ProviderAnthropic,
			Event:    0,
			Reason:   "stream did not contain message_start and message_stop",
		}
	}
	output := text.String()
	if toolInput.Len() > 0 {
		output = toolInput.String()
	}
	return review.ModelResponse{
		Output:       []byte(output),
		Usage:        usage,
		FinishReason: normalizeFinishReason(finishReason),
	}, nil
}
