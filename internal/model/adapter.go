package model

import (
	"net/http"

	"github.com/stormlightlabs/mire/internal/review"
)

// adapterBase owns the dependencies shared by provider adapters. Protocol
// implementations remain responsible for their own request and response wire
// formats.
type adapterBase struct {
	config      RoleConfig
	credentials CredentialResolver
	client      *http.Client
}

func newAdapterBase(
	config RoleConfig,
	credentials CredentialResolver,
	client *http.Client,
) (adapterBase, error) {
	normalized, err := normalizeRoleConfig(config)
	if err != nil {
		return adapterBase{}, err
	}
	if credentials == nil {
		credentials = EnvironmentCredentialResolver{}
	}
	if client == nil {
		client = &http.Client{}
	}
	return adapterBase{config: normalized, credentials: credentials, client: client}, nil
}

func (base adapterBase) metadata(protocol string) review.ModelMetadata {
	return review.ModelMetadata{
		Adapter:    string(base.config.Provider),
		Protocol:   protocol,
		Model:      base.config.Model,
		Redactions: []string{"credential"},
	}
}

var (
	_ review.Model                 = (*OpenAICompatible)(nil)
	_ review.ModelMetadataProvider = (*OpenAICompatible)(nil)
	_ CapabilityDetector           = (*OpenAICompatible)(nil)
	_ review.Model                 = (*Anthropic)(nil)
	_ review.ModelMetadataProvider = (*Anthropic)(nil)
	_ CapabilityDetector           = (*Anthropic)(nil)
)
