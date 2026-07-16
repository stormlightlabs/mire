// Package model contains provider transports for the provider-neutral review
// model contract.
package model

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

// Provider identifies a wire protocol used by a model adapter.
type Provider string

const (
	// ProviderOpenAICompatible uses the OpenAI Chat Completions protocol.
	ProviderOpenAICompatible Provider = "openai-compatible"
	// ProviderAnthropic uses Anthropic's native Messages protocol.
	ProviderAnthropic Provider = "anthropic"
)

// RetryPolicy bounds transport retries. MaxAttempts includes the first
// request, so a value of one disables retries.
type RetryPolicy struct {
	MaxAttempts  int           `json:"max_attempts"`
	InitialDelay time.Duration `json:"initial_delay"`
	MaxDelay     time.Duration `json:"max_delay"`
}

// Budget bounds provider work that can be observed or requested by an
// adapter. Provider token counts are checked after a response is received;
// output bytes are checked while a stream is assembled.
type Budget struct {
	MaxOutputBytes  int   `json:"max_output_bytes,omitempty"`
	MaxInputTokens  int64 `json:"max_input_tokens,omitempty"`
	MaxOutputTokens int64 `json:"max_output_tokens,omitempty"`
	MaxTotalTokens  int64 `json:"max_total_tokens,omitempty"`
}

// RoleConfig selects one provider endpoint without retaining a credential.
// CredentialRef is an opaque lookup reference such as env:OPENAI_API_KEY,
// never the credential value itself.
type RoleConfig struct {
	Provider      Provider                        `json:"provider"`
	BaseURL       string                          `json:"base_url"`
	Model         string                          `json:"model"`
	CredentialRef string                          `json:"credential_ref,omitempty"`
	Timeout       time.Duration                   `json:"timeout,omitempty"`
	Retry         RetryPolicy                     `json:"retry"`
	Budget        Budget                          `json:"budget"`
	Stream        bool                            `json:"stream,omitempty"`
	Capabilities  map[Capability]CapabilityStatus `json:"capabilities,omitempty"`
}

// Config routes roles to one shared configuration or to named aliases. A
// role present in Roles uses the matching Aliases entry; roles not present use
// Shared. The routing map contains aliases, not credentials.
type Config struct {
	Shared      *RoleConfig                 `json:"shared,omitempty"`
	Aliases     map[string]RoleConfig       `json:"aliases,omitempty"`
	Roles       map[review.ModelRole]string `json:"roles,omitempty"`
	Credentials CredentialResolver          `json:"-"`
	HTTPClient  *http.Client                `json:"-"`
}

// CredentialResolver retrieves a credential at request time. Implementations
// must not log or persist the returned value.
type CredentialResolver interface {
	Resolve(context.Context, string) (string, error)
}

// CredentialResolverFunc adapts a function to CredentialResolver.
type CredentialResolverFunc func(context.Context, string) (string, error)

// Resolve implements CredentialResolver.
func (function CredentialResolverFunc) Resolve(ctx context.Context, reference string) (string, error) {
	if function == nil {
		return "", errors.New("credential resolver is nil")
	}
	return function(ctx, reference)
}

// EnvironmentCredentialResolver resolves references with the explicit
// env:NAME form. Requiring the prefix keeps a credential reference distinct
// from a credential value in configuration.
type EnvironmentCredentialResolver struct{}

// Resolve looks up an environment-backed credential reference.
func (EnvironmentCredentialResolver) Resolve(ctx context.Context, reference string) (string, error) {
	if err := contextError(ctx); err != nil {
		return "", err
	}
	reference = strings.TrimSpace(reference)
	if reference == "" {
		return "", nil
	}
	name, ok := strings.CutPrefix(reference, "env:")
	if !ok || name == "" || strings.TrimSpace(name) != name || strings.ContainsAny(name, "=\x00") {
		return "", errors.New("credential reference must use env:NAME")
	}
	value, ok := os.LookupEnv(name)
	if !ok || value == "" {
		return "", fmt.Errorf("credential reference %q is not set", reference)
	}
	return value, nil
}

// Capability identifies one provider feature relevant to available roles.
type Capability string

const (
	CapabilityChatCompletions Capability = "chat_completions"
	CapabilityMessages        Capability = "messages"
	CapabilityStreaming       Capability = "streaming"
	CapabilityStructured      Capability = "structured_output"
	CapabilityToolUse         Capability = "tool_use"
	CapabilityUsage           Capability = "usage"
	CapabilityModelListing    Capability = "model_listing"
)

// CapabilityStatus describes a feature without pretending that an
// OpenAI-compatible endpoint implements every optional feature.
type CapabilityStatus string

const (
	CapabilitySupported   CapabilityStatus = "supported"
	CapabilityUnsupported CapabilityStatus = "unsupported"
	CapabilityUnknown     CapabilityStatus = "unknown"
)

// CapabilityReport is a provider-neutral compatibility report. Unknown is a
// useful result: OpenAI-compatible deployments do not share a capability
// discovery endpoint for streaming, tools, or structured output.
type CapabilityReport struct {
	Provider    Provider                        `json:"provider"`
	BaseURL     string                          `json:"base_url"`
	Model       string                          `json:"model"`
	Features    map[Capability]CapabilityStatus `json:"features"`
	Limitations []string                        `json:"limitations,omitempty"`
	CheckedAt   time.Time                       `json:"checked_at"`
}

// CapabilityDetector reports the features a configured adapter can use.
type CapabilityDetector interface {
	DetectCapabilities(context.Context) (CapabilityReport, error)
}

// Router creates provider adapters for all four review roles.
type Router struct {
	config Config
}

// NewRouter validates model routing without resolving any credentials.
func NewRouter(config Config) (*Router, error) {
	if config.Shared == nil && len(config.Aliases) == 0 {
		return nil, errors.New("create model router: shared configuration or aliases are required")
	}
	for alias, roleConfig := range config.Aliases {
		if strings.TrimSpace(alias) == "" {
			return nil, errors.New("create model router: alias is empty")
		}
		if _, err := normalizeRoleConfig(roleConfig); err != nil {
			return nil, fmt.Errorf("create model router alias %q: %w", alias, err)
		}
	}
	if config.Shared != nil {
		if _, err := normalizeRoleConfig(*config.Shared); err != nil {
			return nil, fmt.Errorf("create model router shared configuration: %w", err)
		}
	}
	for role, alias := range config.Roles {
		if !validRole(role) {
			return nil, fmt.Errorf("create model router: unsupported role %q", role)
		}
		if _, ok := config.Aliases[alias]; !ok {
			return nil, fmt.Errorf("create model router: role %q references unknown alias %q", role, alias)
		}
	}
	if config.Credentials == nil {
		config.Credentials = EnvironmentCredentialResolver{}
	}
	if config.HTTPClient == nil {
		config.HTTPClient = &http.Client{}
	}
	return &Router{config: config}, nil
}

// Model returns the adapter selected for role.
func (router *Router) Model(role review.ModelRole) (review.Model, error) {
	if router == nil {
		return nil, errors.New("create model: router is nil")
	}
	roleConfig, err := router.roleConfig(role)
	if err != nil {
		return nil, err
	}
	return NewModel(roleConfig, router.config.Credentials, router.config.HTTPClient)
}

// Capabilities returns the selected adapter's compatibility report.
func (router *Router) Capabilities(ctx context.Context, role review.ModelRole) (CapabilityReport, error) {
	modelValue, err := router.Model(role)
	if err != nil {
		return CapabilityReport{}, err
	}
	detector, ok := modelValue.(CapabilityDetector)
	if !ok {
		return CapabilityReport{}, errors.New("model adapter does not report capabilities")
	}
	return detector.DetectCapabilities(ctx)
}

// RoleConfig returns the normalized, credential-free configuration for role.
func (router *Router) RoleConfig(role review.ModelRole) (RoleConfig, error) {
	if router == nil {
		return RoleConfig{}, errors.New("read model configuration: router is nil")
	}
	return router.roleConfig(role)
}

func (router *Router) roleConfig(role review.ModelRole) (RoleConfig, error) {
	if !validRole(role) {
		return RoleConfig{}, fmt.Errorf("read model configuration: unsupported role %q", role)
	}
	if alias, ok := router.config.Roles[role]; ok {
		roleConfig, found := router.config.Aliases[alias]
		if !found {
			return RoleConfig{}, fmt.Errorf(
				"read model configuration: role %q references unknown alias %q",
				role,
				alias,
			)
		}
		return normalizeRoleConfig(roleConfig)
	}
	if router.config.Shared == nil {
		return RoleConfig{}, fmt.Errorf("read model configuration: role %q has no shared configuration or alias", role)
	}
	return normalizeRoleConfig(*router.config.Shared)
}

// NewModel constructs one provider adapter from a credential-free role
// configuration.
func NewModel(config RoleConfig, credentials CredentialResolver, client *http.Client) (review.Model, error) {
	normalized, err := normalizeRoleConfig(config)
	if err != nil {
		return nil, err
	}
	if credentials == nil {
		credentials = EnvironmentCredentialResolver{}
	}
	if client == nil {
		client = &http.Client{}
	}
	switch normalized.Provider {
	case ProviderOpenAICompatible:
		return NewOpenAICompat(normalized, credentials, client)
	case ProviderAnthropic:
		return NewAnthropic(normalized, credentials, client)
	default:
		return nil, fmt.Errorf("create model: provider %q is unsupported", normalized.Provider)
	}
}

func normalizeRoleConfig(config RoleConfig) (RoleConfig, error) {
	config.Provider = Provider(strings.TrimSpace(string(config.Provider)))
	config.BaseURL = strings.TrimRight(strings.TrimSpace(config.BaseURL), "/")
	config.Model = strings.TrimSpace(config.Model)
	config.CredentialRef = strings.TrimSpace(config.CredentialRef)
	if config.Provider != ProviderOpenAICompatible && config.Provider != ProviderAnthropic {
		return RoleConfig{}, fmt.Errorf("provider %q is unsupported", config.Provider)
	}
	if config.BaseURL == "" {
		return RoleConfig{}, errors.New("model base URL is required")
	}
	if config.Model == "" {
		return RoleConfig{}, errors.New("model name is required")
	}
	if config.Timeout <= 0 {
		config.Timeout = review.DefaultRetryPolicy.Timeout
	}
	if config.Retry.MaxAttempts < 1 {
		config.Retry.MaxAttempts = 2
	}
	if config.Retry.MaxAttempts > 8 {
		return RoleConfig{}, errors.New("model retry attempts cannot exceed 8")
	}
	if config.Retry.InitialDelay <= 0 {
		config.Retry.InitialDelay = 100 * time.Millisecond
	}
	if config.Retry.MaxDelay <= 0 {
		config.Retry.MaxDelay = 2 * time.Second
	}
	if config.Retry.MaxDelay < config.Retry.InitialDelay {
		config.Retry.MaxDelay = config.Retry.InitialDelay
	}
	if config.Budget.MaxOutputBytes < 0 || config.Budget.MaxInputTokens < 0 || config.Budget.MaxOutputTokens < 0 ||
		config.Budget.MaxTotalTokens < 0 {
		return RoleConfig{}, errors.New("model budgets cannot be negative")
	}
	for capability, status := range config.Capabilities {
		if status != CapabilitySupported && status != CapabilityUnsupported && status != CapabilityUnknown {
			return RoleConfig{}, fmt.Errorf("capability %q has unsupported status %q", capability, status)
		}
	}
	return config, nil
}

func validRole(role review.ModelRole) bool {
	switch role {
	case review.ModelRolePlanner, review.ModelRoleReviewer, review.ModelRoleVerifier, review.ModelRoleChat:
		return true
	default:
		return false
	}
}

func contextError(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	return ctx.Err()
}
