package review

import (
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/shared"
)

// ModelRunOptions contains configuration shared by every model-backed review
// role. Role option types embed it and retain their own budgets, stores, and
// snapshot-specific dependencies.
type ModelRunOptions struct {
	Retry                 RetryPolicy
	Adapter               string
	Protocol              string
	PromptTemplateVersion string
	Model                 string
	Parameters            map[string]any
	Redactions            []string
	Now                   func() time.Time
}

// normalize applies bounded defaults, copies caller-owned collections, and
// incorporates optional credential-free adapter metadata for one role.
func (options ModelRunOptions) normalize(model Model, promptTemplateVersion string) ModelRunOptions {
	options.Retry = normalizeRetryPolicy(options.Retry)
	options.Adapter = strings.TrimSpace(options.Adapter)
	if options.Adapter == "" {
		options.Adapter = "unknown"
	}
	options.Protocol = strings.TrimSpace(options.Protocol)
	if options.Protocol == "" {
		options.Protocol = "provider-neutral"
	}
	options.PromptTemplateVersion = strings.TrimSpace(options.PromptTemplateVersion)
	if options.PromptTemplateVersion == "" {
		options.PromptTemplateVersion = promptTemplateVersion
	}
	options.Model = strings.TrimSpace(options.Model)
	options.Parameters = shared.CloneMap(options.Parameters)
	options.Redactions = shared.UniqueStrings(options.Redactions)
	if options.Now == nil {
		options.Now = time.Now
	}

	if metadataProvider, ok := model.(ModelMetadataProvider); ok {
		metadata := metadataProvider.Metadata()
		if options.Adapter == "unknown" && strings.TrimSpace(metadata.Adapter) != "" {
			options.Adapter = strings.TrimSpace(metadata.Adapter)
		}
		if options.Protocol == "provider-neutral" && strings.TrimSpace(metadata.Protocol) != "" {
			options.Protocol = strings.TrimSpace(metadata.Protocol)
		}
		if options.Model == "" {
			options.Model = strings.TrimSpace(metadata.Model)
		}
		options.Redactions = shared.UniqueStrings(append(options.Redactions, metadata.Redactions...))
	}
	return options
}

func normalizeRetryPolicy(policy RetryPolicy) RetryPolicy {
	if policy.MaxAttempts < 1 {
		policy.MaxAttempts = DefaultRetryPolicy.MaxAttempts
	}
	if policy.RepairAttempts < 0 {
		policy.RepairAttempts = 0
	}
	if policy.Timeout <= 0 {
		policy.Timeout = DefaultRetryPolicy.Timeout
	}
	if policy.MaxOutputBytes <= 0 {
		policy.MaxOutputBytes = DefaultRetryPolicy.MaxOutputBytes
	}
	return policy
}
