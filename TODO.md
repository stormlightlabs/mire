# Struct refactoring opportunities

This file tracks opportunities found during a review of MIRE's Go struct
definitions. Each change should preserve existing JSON shapes, database
representations, digest inputs, validation rules, and package boundaries unless
the change explicitly includes a compatible migration.

- [x] Embed `db.RepositoryIdentity` in `db.Repository`. The persisted repository
  repeats `CanonicalIdentity`, `DisplayName`, and `DiscoveredGitDir` exactly.
  Keep `ID` and `CreatedAt` on `Repository`, and update scanners and constructors
  to populate the embedded identity.

- [x] Introduce a shared model-run options type for `PlannerOptions`,
  `ReviewerOpts`, `VerifierOptions`, and `ChatOptions`. It should own `Retry`,
  `Adapter`, `Protocol`, `PromptTemplateVersion`, `Model`, `Parameters`,
  `Redactions`, and `Now`. Embed it only if the resulting nested keyed literals
  remain clear at call sites.

- [x] Move common model-run option behavior with the shared type. Normalize
  retry limits, adapter and protocol defaults, clocks, model metadata, parameter
  copies, and redactions in one place. Let each role supply its own prompt
  template version and role-specific budgets, retriever, store, and round data.

- [x] Centralize `RunRecord` and `RunProvenance` construction. Planner,
  reviewer, verifier, and chat currently assemble the same identity, timestamps,
  retry count, provider metadata, parameters, manifest digest, and redactions.
  A constructor should require the role-specific input digest and pass name
  rather than filling them implicitly.

- [x] Group the repeated tree state in `snapshot.Capture`. Define a named type
  containing an OID, entries, and manifest digest, then use named `Base`,
  `Target`, `Head`, `Index`, and `Worktree` fields. Keep comparison-wide values
  such as the requested comparison, effective base, merge base, object format,
  policy hashes, capture time, changes, and overall manifest digest on
  `Capture`.

- [x] Reconcile `snapshot.Capture.Layers` with the proposed named tree fields.
  There should be one authoritative representation of layer identity and
  manifest metadata. Derived layer lists should be built deterministically
  instead of allowing the named fields and `Layers` slice to disagree.

- [x] Extract the common location fields shared by `review.Evidence` and
  `review.VerificationPathStep`: `Kind`, `Summary`, `SnapshotID`, `Anchors`,
  `ArtifactDigest`, and `OutputPointer`. Embed a small evidence-location type and
  leave relation, producing run, independence, concreteness, and materiality on
  `Evidence`.

- [x] Extract the common candidate content shared by `review.Candidate` and
  `review.ChatCandidateProposal`: claim, impact, category, severity, confidence,
  anchors, and rationale. Keep reviewer source identity and chat proposal
  lifecycle behavior outside the shared value. Route both paths through the
  same content validation without allowing a chat proposal to become a finding
  implicitly.

- [x] Consider a shared review-scope value for the repeated `SessionID`,
  `RoundID`, and `SnapshotID` tuple. Candidate users include `RunRecord`,
  `VerificationRecord`, `ChatBinding`, `ChatMessage`, `ChatTurnRequest`, and
  `FindingRevision`. Preserve each field's current JSON tag and required versus
  optional behavior; do not apply this abstraction to types whose scope is
  partial or has different omission rules.

- [ ] Extract safe retrieved-artifact metadata from `review.RetrievedArtifact`.
  The metadata can contain identity, run and pass provenance, kind, path,
  relation, hunk IDs, digest, size, exclusion, and truncation fields while the
  content remains a separate private field. Evaluate whether
  `export.ArtifactDescriptor` can reuse only this safe metadata type.

- [ ] Share the private adapter dependencies used by `model.OpenAICompatible`
  and `model.Anthropic`: role configuration, credential resolver, and HTTP
  client. An unexported embedded adapter base can also own constructor defaults
  and common metadata behavior while protocol-specific request code stays on
  each adapter.

- [ ] Audit anonymous versus named `RunRecord` composition. Keep
  `VerificationRunRecord` flattened only if its current JSON representation is
  intentional, and keep chat and pass projections nested where their schemas
  require a `run` object. Add serialization tests before changing any embedding.

- [ ] Add compile-time and serialization coverage for every extracted type.
  Tests should compare JSON byte-for-byte where values feed canonical exports or
  digests, exercise database round trips, and confirm that zero values and
  `omitempty` behavior have not changed.

## Boundaries to preserve

- Keep snapshot domain types, database rows, server DTOs, and export descriptors
  distinct when they expose different data or use different representations.
  Similar field names alone are not a reason to couple those packages.
- Do not embed `snapshot.Entry`, `review.RetrievedArtifact`, or another richer
  source type into an export descriptor. Export types must make omitted bytes
  and private state impossible to serialize accidentally.
- Keep operation, round, model-run, and verification statuses as separate typed
  state machines. Their timestamps and status fields look similar but permit
  different transitions.
- Keep `review.Anchor` as the single location type used by candidates,
  verification evidence, chat references, and findings.
- Keep durable findings, reviewer candidates, and chat proposals as separate
  lifecycle types even when they reuse a smaller content value.
