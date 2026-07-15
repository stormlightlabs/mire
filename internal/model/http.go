package model

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

const (
	maxProviderErrorBytes = 64 << 10
	maxSSELineBytes       = 1 << 20
)

var (
	secretAssignmentPattern = regexp.MustCompile(`(?i)(authorization|api[-_ ]?key|token|secret|password)\s*[:=]\s*[^\s,;]+`)
	bearerPattern           = regexp.MustCompile(`(?i)\bBearer\s+[^\s,;]+`)
	apiKeyPattern           = regexp.MustCompile(`\bsk-[A-Za-z0-9_-]+\b`)
)

// ProviderError is a sanitized provider or HTTP failure. It intentionally
// retains status and retry guidance but never retains response bodies or
// authorization values.
type ProviderError struct {
	Provider   Provider
	Operation  string
	StatusCode int
	Code       string
	Type       string
	Message    string
	Retryable  bool
	RetryAfter time.Duration
}

// Error returns a credential-free provider error description.
func (err *ProviderError) Error() string {
	if err == nil {
		return ""
	}
	parts := []string{string(err.Provider) + " provider error"}
	if err.Operation != "" {
		parts = append(parts, err.Operation)
	}
	if err.StatusCode > 0 {
		parts = append(parts, fmt.Sprintf("status %d", err.StatusCode))
	}
	if err.Code != "" {
		parts = append(parts, "code "+sanitizeText(err.Code, ""))
	}
	if err.Message != "" {
		parts = append(parts, sanitizeText(err.Message, ""))
	}
	return strings.Join(parts, ": ")
}

// BudgetError identifies a provider response that exceeded a configured
// transport budget. Its output is deliberately not retained in the response.
type BudgetError struct {
	Kind  string
	Value int64
	Limit int64
}

// Error returns the bounded budget failure.
func (err *BudgetError) Error() string {
	if err == nil {
		return ""
	}
	return fmt.Sprintf("model %s budget exceeded: %d > %d", err.Kind, err.Value, err.Limit)
}

// MalformedResponseError identifies a provider response that cannot be
// translated into the common model response without retaining raw data.
type MalformedResponseError struct {
	Provider  Provider
	Operation string
	Reason    string
}

// Error returns the safe malformed-response description.
func (err *MalformedResponseError) Error() string {
	if err == nil {
		return ""
	}
	return fmt.Sprintf("%s %s response is malformed: %s", err.Provider, err.Operation, sanitizeText(err.Reason, ""))
}

// MalformedStreamError identifies a malformed or incomplete SSE response.
type MalformedStreamError struct {
	Provider Provider
	Event    int
	Reason   string
}

// Error returns the safe stream failure without including partial output.
func (err *MalformedStreamError) Error() string {
	if err == nil {
		return ""
	}
	return fmt.Sprintf("%s streaming response is malformed at event %d: %s", err.Provider, err.Event, sanitizeText(err.Reason, ""))
}

type transportParser func(io.Reader) (review.ModelResponse, error)

func execute(ctx context.Context, client *http.Client, provider Provider, operation, endpoint, body, secret string, headers http.Header, config RoleConfig, parse transportParser) (review.ModelResponse, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if client == nil {
		return review.ModelResponse{}, errors.New("model request: HTTP client is nil")
	}
	callCtx, cancel := context.WithTimeout(ctx, config.Timeout)
	defer cancel()
	bodyBytes := []byte(body)
	for attempt := 1; attempt <= config.Retry.MaxAttempts; attempt++ {
		if err := callCtx.Err(); err != nil {
			return review.ModelResponse{}, err
		}
		request, err := http.NewRequestWithContext(callCtx, http.MethodPost, endpoint, bytes.NewReader(bodyBytes))
		if err != nil {
			return review.ModelResponse{}, fmt.Errorf("create %s request: %w", provider, err)
		}
		request.Header.Set("Content-Type", "application/json")
		request.Header.Set("Accept", "application/json")
		for key, values := range headers {
			for _, value := range values {
				request.Header.Add(key, value)
			}
		}
		response, requestErr := client.Do(request)
		if requestErr != nil {
			if callCtx.Err() != nil {
				return review.ModelResponse{}, callCtx.Err()
			}
			if attempt == config.Retry.MaxAttempts {
				return review.ModelResponse{}, fmt.Errorf("%s request: %w", provider, requestErr)
			}
			if err := waitRetry(callCtx, config.Retry, attempt, 0); err != nil {
				return review.ModelResponse{}, err
			}
			continue
		}

		if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
			providerErr := decodeProviderError(provider, operation, response.StatusCode, response.Header, response.Body, secret)
			_ = response.Body.Close()
			if providerErr.Retryable && attempt < config.Retry.MaxAttempts {
				if err := waitRetry(callCtx, config.Retry, attempt, providerErr.RetryAfter); err != nil {
					return review.ModelResponse{}, err
				}
				continue
			}
			return review.ModelResponse{}, providerErr
		}

		result, parseErr := parse(response.Body)
		closeErr := response.Body.Close()
		if parseErr != nil {
			if closeErr != nil {
				return review.ModelResponse{}, fmt.Errorf("parse %s response: %w; close response: %v", provider, parseErr, closeErr)
			}
			return review.ModelResponse{}, parseErr
		}
		if closeErr != nil {
			return review.ModelResponse{}, fmt.Errorf("close %s response: %w", provider, closeErr)
		}
		if err := enforceUsageBudget(result.Usage, config.Budget); err != nil {
			return review.ModelResponse{}, err
		}
		return result, nil
	}
	return review.ModelResponse{}, errors.New("model request exhausted retry attempts")
}

func waitRetry(ctx context.Context, policy RetryPolicy, attempt int, retryAfter time.Duration) error {
	delay := policy.InitialDelay
	for index := 1; index < attempt; index++ {
		if delay >= policy.MaxDelay/2 && policy.MaxDelay > 0 {
			delay = policy.MaxDelay
			break
		}
		delay *= 2
	}
	if retryAfter > delay {
		delay = retryAfter
	}
	if policy.MaxDelay > 0 && delay > policy.MaxDelay {
		delay = policy.MaxDelay
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func enforceUsageBudget(usage review.Usage, budget Budget) error {
	if budget.MaxInputTokens > 0 && usage.InputTokens > budget.MaxInputTokens {
		return &BudgetError{Kind: "input tokens", Value: usage.InputTokens, Limit: budget.MaxInputTokens}
	}
	if budget.MaxOutputTokens > 0 && usage.OutputTokens > budget.MaxOutputTokens {
		return &BudgetError{Kind: "output tokens", Value: usage.OutputTokens, Limit: budget.MaxOutputTokens}
	}
	total := usage.TotalTokens
	if total == 0 {
		total = usage.InputTokens + usage.OutputTokens
	}
	if budget.MaxTotalTokens > 0 && total > budget.MaxTotalTokens {
		return &BudgetError{Kind: "total tokens", Value: total, Limit: budget.MaxTotalTokens}
	}
	return nil
}

func decodeProviderError(provider Provider, operation string, status int, headers http.Header, body io.Reader, secret string) *ProviderError {
	data, _ := io.ReadAll(io.LimitReader(body, maxProviderErrorBytes))
	var envelope struct {
		Error   json.RawMessage `json:"error"`
		Message string          `json:"message"`
		Type    string          `json:"type"`
		Code    string          `json:"code"`
	}
	_ = json.Unmarshal(data, &envelope)
	message := envelope.Message
	if len(envelope.Error) > 0 && string(envelope.Error) != "null" {
		var nested struct {
			Message string `json:"message"`
			Type    string `json:"type"`
			Code    string `json:"code"`
		}
		if json.Unmarshal(envelope.Error, &nested) == nil {
			if message == "" {
				message = nested.Message
			}
			if envelope.Type == "" {
				envelope.Type = nested.Type
			}
			if envelope.Code == "" {
				envelope.Code = nested.Code
			}
		}
	}
	if message == "" {
		message = http.StatusText(status)
	}
	return &ProviderError{Provider: provider, Operation: operation, StatusCode: status,
		Code: sanitizeText(envelope.Code, secret), Type: sanitizeText(envelope.Type, secret),
		Message: sanitizeText(message, secret), Retryable: retryableStatus(status), RetryAfter: parseRetryAfter(headers)}
}

func retryableStatus(status int) bool {
	return status == http.StatusRequestTimeout || status == http.StatusConflict || status == http.StatusTooEarly || status == http.StatusTooManyRequests || status >= 500
}

func parseRetryAfter(headers http.Header) time.Duration {
	value := strings.TrimSpace(headers.Get("Retry-After"))
	if value == "" {
		return 0
	}
	if seconds, err := strconv.Atoi(value); err == nil && seconds >= 0 {
		return time.Duration(seconds) * time.Second
	}
	if timestamp, err := http.ParseTime(value); err == nil {
		if delay := time.Until(timestamp); delay > 0 {
			return delay
		}
	}
	return 0
}

func endpoint(baseURL, path string) (string, error) {
	parsed, err := url.Parse(strings.TrimSpace(baseURL))
	if err != nil || parsed.Scheme == "" || parsed.Host == "" || parsed.User != nil || parsed.RawQuery != "" || parsed.Fragment != "" {
		return "", errors.New("model base URL must be an absolute URL without credentials, query, or fragment")
	}
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return "", fmt.Errorf("model base URL scheme %q is unsupported", parsed.Scheme)
	}
	return strings.TrimRight(parsed.String(), "/") + "/" + strings.TrimLeft(path, "/"), nil
}

func resolveCredential(ctx context.Context, resolver CredentialResolver, reference string) (string, error) {
	if err := contextError(ctx); err != nil {
		return "", err
	}
	if strings.TrimSpace(reference) == "" {
		return "", nil
	}
	if resolver == nil {
		return "", errors.New("model credential resolver is nil")
	}
	value, err := resolver.Resolve(ctx, reference)
	if err != nil {
		return "", fmt.Errorf("resolve model credential: %w", sanitizeError(err, ""))
	}
	if value == "" {
		return "", errors.New("resolved model credential is empty")
	}
	return value, nil
}

func sanitizeError(err error, secret string) error {
	if err == nil {
		return nil
	}
	return errors.New(sanitizeText(err.Error(), secret))
}

func sanitizeText(value, secret string) string {
	if secret != "" {
		value = strings.ReplaceAll(value, secret, "[REDACTED]")
	}
	value = secretAssignmentPattern.ReplaceAllString(value, "$1=[REDACTED]")
	value = bearerPattern.ReplaceAllString(value, "Bearer [REDACTED]")
	value = apiKeyPattern.ReplaceAllString(value, "[REDACTED]")
	if len(value) > 512 {
		value = value[:512] + "…"
	}
	return value
}

type sseEvent struct {
	Name string
	Data string
}

func readSSE(reader io.Reader, provider Provider, callback func(int, sseEvent) error) error {
	scanner := bufio.NewScanner(reader)
	scanner.Buffer(make([]byte, 4<<10), maxSSELineBytes)
	var eventName string
	var data []string
	eventNumber := 0
	dispatch := func() error {
		if len(data) == 0 {
			return nil
		}
		eventNumber++
		event := sseEvent{Name: eventName, Data: strings.Join(data, "\n")}
		eventName = ""
		data = nil
		return callback(eventNumber, event)
	}
	for scanner.Scan() {
		line := strings.TrimSuffix(scanner.Text(), "\r")
		if line == "" {
			if err := dispatch(); err != nil {
				return err
			}
			continue
		}
		if strings.HasPrefix(line, ":") {
			continue
		}
		if value, ok := strings.CutPrefix(line, "event:"); ok {
			eventName = strings.TrimSpace(value)
			continue
		}
		if value, ok := strings.CutPrefix(line, "data:"); ok {
			data = append(data, strings.TrimPrefix(value, " "))
			continue
		}
		return &MalformedStreamError{Provider: provider, Event: eventNumber + 1, Reason: "unexpected SSE field"}
	}
	if err := scanner.Err(); err != nil {
		return &MalformedStreamError{Provider: provider, Event: eventNumber + 1, Reason: "SSE frame exceeds the configured parser limit or could not be read"}
	}
	if err := dispatch(); err != nil {
		return err
	}
	return nil
}

func normalizeFinishReason(reason string) string {
	switch strings.ToLower(strings.TrimSpace(reason)) {
	case "", "stop", "end_turn":
		return "completed"
	case "length", "max_tokens":
		return "max_tokens"
	case "tool_calls", "function_call", "tool_use":
		return "tool_use"
	case "content_filter", "refusal":
		return "refused"
	case "stop_sequence":
		return "stop_sequence"
	case "pause_turn":
		return "paused"
	default:
		return sanitizeText(reason, "")
	}
}

func contentText(raw json.RawMessage) (string, error) {
	if len(raw) == 0 || string(raw) == "null" {
		return "", nil
	}
	var text string
	if err := json.Unmarshal(raw, &text); err == nil {
		return text, nil
	}
	var blocks []struct {
		Type string `json:"type"`
		Text string `json:"text"`
	}
	if err := json.Unmarshal(raw, &blocks); err != nil {
		return "", err
	}
	var result strings.Builder
	for _, block := range blocks {
		if block.Type == "text" || block.Type == "output_text" || block.Type == "" {
			result.WriteString(block.Text)
		}
	}
	return result.String(), nil
}

func requireJSONEOF(decoder *json.Decoder) error {
	var extra any
	if err := decoder.Decode(&extra); !errors.Is(err, io.EOF) {
		return errors.New("trailing JSON")
	}
	return nil
}
