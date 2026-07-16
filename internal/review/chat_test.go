package review

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"
)

func TestChatContextRequiresAnImmutableReference(t *testing.T) {
	t.Parallel()

	if _, err := NormalizeChatContext(ChatContext{}, "snapshot-1"); err == nil {
		t.Fatal("NormalizeChatContext() accepted an empty context")
	}
	if _, err := NormalizeChatContext(ChatContext{References: []ChatReference{{DiffAnchor: &Anchor{
		SnapshotID: "snapshot-1", Side: "target", Layer: "target", Path: "src/a.go", HunkID: "src/a.go#1",
	}}}}, "snapshot-1"); err == nil {
		t.Fatal("NormalizeChatContext() accepted a line-only anchor")
	}
	if _, err := NormalizeChatContext(
		ChatContext{
			References: []ChatReference{{FindingRevision: &FindingRevisionRef{FindingID: "finding-1", Revision: 1}}},
		},
		"snapshot-1",
	); err != nil {
		t.Fatalf("NormalizeChatContext() rejected a finding revision: %v", err)
	}
}

func TestChatResponseKeepsCandidateAndVerificationAsExplicitProposals(t *testing.T) {
	t.Parallel()

	binding := testChatBinding()
	canonicalContext, err := NormalizeChatContext(binding.Context, binding.SnapshotID)
	if err != nil {
		t.Fatal(err)
	}
	binding.Context = canonicalContext
	response := ChatResponse{
		SchemaVersion: ChatResponseSchemaVersion,
		Body:          "I found a possible issue.",
		CandidateProposal: &ChatCandidateProposal{
			Claim:      "The branch accepts invalid input.",
			Impact:     "Invalid state can escape.",
			Category:   "correctness",
			Severity:   "high",
			Confidence: 0.8,
			Anchors:    []Anchor{*binding.Context.Primary.DiffAnchor},
		},
		VerificationRequest: &ChatVerificationRequest{
			FindingRevision: FindingRevisionRef{FindingID: "finding-1", Revision: 2},
			Reason:          "Please re-check the guard path.",
		},
	}
	if err := ValidateChatResponse(response, binding); err != nil {
		t.Fatalf("ValidateChatResponse() error = %v", err)
	}
	response.CandidateProposal.Anchors[0].SnapshotID = "other-snapshot"
	if err := ValidateChatResponse(response, binding); err == nil {
		t.Fatal("ValidateChatResponse() accepted an anchor from another snapshot")
	}
}

func TestRunChatPersistsBindingBeforeModelAndKeepsAssistantOnPrimary(t *testing.T) {
	t.Parallel()

	clock := time.Date(2026, time.July, 15, 18, 0, 0, 0, time.UTC)
	binding := ChatBinding{
		SessionID:      "session-1",
		RoundID:        "round-1",
		SnapshotID:     "snapshot-1",
		SnapshotDigest: "manifest-1",
		Context: ChatContext{
			References: []ChatReference{
				{
					DiffAnchor: &Anchor{
						SnapshotID: "snapshot-1",
						Side:       "target",
						Layer:      "target",
						Path:       "src/a.go",
						HunkID:     "src/a.go#1",
						HunkDigest: "hunk-1",
					},
				},
			},
		},
	}
	store := &chatMemoryStore{binding: binding}
	model := &chatResponseModel{
		responses: []ModelResponse{{Output: chatResponseBytes(t, "The selected hunk is safely scoped.")}},
	}
	result, err := RunChat(context.Background(), ChatTurnRequest{
		SessionID: binding.SessionID, RoundID: binding.RoundID, SnapshotID: binding.SnapshotID,
		Body: "What does this hunk change?", Context: binding.Context,
	}, model, ChatOptions{
		ModelRunOptions: ModelRunOptions{
			Retry:    RetryPolicy{MaxAttempts: 1},
			Adapter:  "fixture",
			Protocol: "fixture/v1",
			Now:      func() time.Time { return clock },
		},
		Store: store,
		Retriever: chatRetrieverFunc(
			func(_ context.Context, _ ChatTurnRequest, binding ChatBinding) ([]ChatRetrievedArtifact, error) {
				return []ChatRetrievedArtifact{
					{SnapshotID: binding.SnapshotID, Kind: "source", Path: "src/a.go", Content: "package a"},
				}, nil
			},
		),
	})
	if err != nil {
		t.Fatalf("RunChat() error = %v", err)
	}
	if len(store.messages) != 2 || store.messages[0].Role != MessageRoleUser ||
		store.messages[1].Role != MessageRoleAssistant {
		t.Fatalf("messages = %#v, want one user and one assistant", store.messages)
	}
	if len(store.runs) != 1 || store.runs[0].Run.Status != RunStatusComplete {
		t.Fatalf("runs = %#v", store.runs)
	}
	if len(store.runs[0].Input.Artifacts) != 1 || store.runs[0].Input.Artifacts[0].Digest == "" {
		t.Fatalf("run input artifacts = %#v", store.runs[0].Input.Artifacts)
	}
	if len(store.messages[1].Context.References) != 1 ||
		store.messages[1].Context.Primary.Kind != ChatReferenceDiffAnchor {
		t.Fatalf("assistant context = %#v, want the initiating primary binding", store.messages[1].Context)
	}
	if result.Assistant == nil || result.Assistant.ProducerRunID != store.runs[0].Run.ID {
		t.Fatalf("result = %#v, want assistant provenance", result)
	}
}

func TestRunChatRepairsMalformedOutputAndLeavesFailedRunsDurable(t *testing.T) {
	t.Parallel()

	binding := testChatBinding()
	store := &chatMemoryStore{binding: binding}
	model := &chatResponseModel{
		responses: []ModelResponse{{Output: []byte("not-json")}, {Output: chatResponseBytes(t, "repaired")}},
	}
	result, err := RunChat(
		context.Background(),
		testChatRequest(binding),
		model,
		ChatOptions{
			ModelRunOptions: ModelRunOptions{Retry: RetryPolicy{MaxAttempts: 2, RepairAttempts: 1}},
			Store:           store,
		},
	)
	if err != nil || result.Run.Run.Status != RunStatusComplete || model.calls != 2 {
		t.Fatalf("repaired result = %#v, error = %v, calls = %d", result, err, model.calls)
	}

	store = &chatMemoryStore{binding: binding}
	model = &chatResponseModel{responses: []ModelResponse{{Output: []byte("bad-1")}, {Output: []byte("bad-2")}}}
	result, err = RunChat(
		context.Background(),
		testChatRequest(binding),
		model,
		ChatOptions{
			ModelRunOptions: ModelRunOptions{Retry: RetryPolicy{MaxAttempts: 2, RepairAttempts: 1}},
			Store:           store,
		},
	)
	var chatErr *ChatError
	if !errors.As(err, &chatErr) || chatErr.Status != RunStatusFailed || result.Run.Run.Status != RunStatusFailed {
		t.Fatalf("failed result = %#v, error = %v", result, err)
	}
	if len(store.messages) != 1 || len(store.runs) != 1 || store.runs[0].RetainedOutput != "bad-2" {
		t.Fatalf(
			"failed durable state: messages=%d runs=%d output=%q",
			len(store.messages),
			len(store.runs),
			store.runs[0].RetainedOutput,
		)
	}
}

func TestRunChatCancellationPersistsTerminalStatus(t *testing.T) {
	t.Parallel()

	binding := testChatBinding()
	store := &chatMemoryStore{binding: binding}
	model := &chatResponseModel{
		respectContext: true,
		responses:      []ModelResponse{{Output: chatResponseBytes(t, "never")}},
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	result, err := RunChat(
		ctx,
		testChatRequest(binding),
		model,
		ChatOptions{ModelRunOptions: ModelRunOptions{Retry: RetryPolicy{MaxAttempts: 1}}, Store: store},
	)
	var chatErr *ChatError
	if !errors.As(err, &chatErr) || chatErr.Status != RunStatusCancelled ||
		result.Run.Run.Status != RunStatusCancelled {
		t.Fatalf("cancel result = %#v, error = %v", result, err)
	}
	if len(store.runs) != 1 || store.runs[0].Run.Status != RunStatusCancelled {
		t.Fatalf("runs after cancellation = %#v", store.runs)
	}
}

type chatMemoryStore struct {
	binding  ChatBinding
	messages []ChatMessage
	runs     []ChatRunRecord
}

func (store *chatMemoryStore) ValidateChatBinding(_ context.Context, binding ChatBinding) (ChatBinding, error) {
	if binding.SessionID != store.binding.SessionID || binding.RoundID != store.binding.RoundID ||
		binding.SnapshotID != store.binding.SnapshotID {
		return ChatBinding{}, errors.New("binding mismatch")
	}
	canonical := store.binding
	canonical.Context = binding.Context
	return NormalizeChatBinding(canonical)
}

func (store *chatMemoryStore) SaveChatMessage(_ context.Context, message ChatMessage) (ChatMessage, error) {
	if err := ValidateChatMessage(message); err != nil {
		return ChatMessage{}, err
	}
	store.messages = append(store.messages, message)
	return message, nil
}

func (store *chatMemoryStore) ListChatMessages(_ context.Context, _ string) ([]ChatMessage, error) {
	return append([]ChatMessage(nil), store.messages...), nil
}

func (store *chatMemoryStore) CreateChatRun(_ context.Context, run ChatRunRecord) (ChatRunRecord, error) {
	if err := ValidateChatRunRecord(run); err != nil {
		return ChatRunRecord{}, err
	}
	store.runs = append(store.runs, run)
	return run, nil
}

func (store *chatMemoryStore) UpdateChatRun(_ context.Context, run ChatRunRecord) error {
	for index := range store.runs {
		if store.runs[index].Run.ID == run.Run.ID {
			store.runs[index] = run
			return ValidateChatRunRecord(run)
		}
	}
	return errors.New("run not found")
}

type chatResponseModel struct {
	responses      []ModelResponse
	calls          int
	respectContext bool
}

type chatRetrieverFunc func(context.Context, ChatTurnRequest, ChatBinding) ([]ChatRetrievedArtifact, error)

func (retriever chatRetrieverFunc) Retrieve(
	ctx context.Context,
	request ChatTurnRequest,
	binding ChatBinding,
) ([]ChatRetrievedArtifact, error) {
	return retriever(ctx, request, binding)
}

func (model *chatResponseModel) Complete(ctx context.Context, _ ModelRequest) (ModelResponse, error) {
	if model.respectContext {
		select {
		case <-ctx.Done():
			return ModelResponse{}, ctx.Err()
		default:
		}
	}
	response := model.responses[model.calls]
	model.calls++
	return response, nil
}

func testChatBinding() ChatBinding {
	return ChatBinding{
		SessionID:      "session-1",
		RoundID:        "round-1",
		SnapshotID:     "snapshot-1",
		SnapshotDigest: "manifest-1",
		Context: ChatContext{
			References: []ChatReference{
				{
					DiffAnchor: &Anchor{
						SnapshotID: "snapshot-1",
						Side:       "target",
						Layer:      "target",
						Path:       "src/a.go",
						HunkID:     "src/a.go#1",
						HunkDigest: "hunk-1",
					},
				},
			},
		},
	}
}

func testChatRequest(binding ChatBinding) ChatTurnRequest {
	return ChatTurnRequest{
		SessionID:  binding.SessionID,
		RoundID:    binding.RoundID,
		SnapshotID: binding.SnapshotID,
		Body:       "Explain the selected change.",
		Context:    binding.Context,
	}
}

func chatResponseBytes(t *testing.T, body string) []byte {
	t.Helper()
	data, err := json.Marshal(ChatResponse{SchemaVersion: ChatResponseSchemaVersion, Body: body})
	if err != nil {
		t.Fatal(err)
	}
	return data
}
