---
title: "CodeRabbit Learnings"
sources:
  - https://docs.coderabbit.ai/knowledge-base/learnings
author: CodeRabbit Documentation
captured: 2026-07-14
tags:
  - code-review
  - persistent-memory
  - review-policy
  - preference-learning
---

## Summary

CodeRabbit Learnings persist natural-language review preferences derived from feedback, then retrieve them by repository or organization scope for future reviews, with provenance, administration, and optional approval controls.

## Source Boundary

- **Learnings documentation:** Describes CodeRabbit's proprietary memory behavior and recommended operating practices.
- **Review instructions and code guidelines:** Separate, more formal policy mechanisms linked by the page but not covered in depth here.

The documentation describes vendor behavior rather than an open memory format, and it does not evaluate whether applying learnings improves review accuracy.

## Key Ideas

- **Feedback can become durable context:** A correction made during review may be stored as a reusable natural-language preference.
- **Patterns differ from exceptions:** One-off decisions should be resolved locally rather than promoted into future behavior.
- **Rationale improves transfer:** A learning that records why a rule exists can apply more sensibly to similar but non-identical cases.
- **Scope prevents leakage:** Repository-local and organization-wide conventions need different boundaries.
- **Memory needs governance:** Provenance, approval, editing, deletion, export, usage tracking, and periodic maintenance are product requirements.
- **Instruction precedence matters:** Formal path instructions and coding guidelines can override or deprioritize learned preferences.

## What Learnings Do

- Store natural-language review preferences in an organization-associated internal database
- Create records from review conversations, imported files, or the review pipeline
- Load applicable records before generating a future comment
- Associate records with repository and source metadata
- Track creation, updates, last use, and usage count
- Support semantic search, editing, deletion, CSV export, and import
- Allow organizations or repositories to opt out of long-term knowledge storage
- Optionally hold new chat-sourced records for administrator review

## How It Works

### Creation

A reviewer replies to a specific CodeRabbit comment with a correction and rationale. CodeRabbit may translate that exchange into a self-instruction, disclose that a learning was created, and retain source metadata such as pull request, file, and user.

The page recommends replying at the most specific comment location available, deciding whether feedback represents a recurring pattern, and explaining the reason rather than only the preferred action.

### Retrieval and Precedence

Before adding a future comment, CodeRabbit loads learnings applicable to the configured scope and treats them as context or instructions. Formal path instructions precede learnings. Conflicts or contextual overlap can cause a model to ignore or inconsistently apply records.

### Scope

| Scope    | Behavior                                                                                 |
| -------- | ---------------------------------------------------------------------------------------- |
| `local`  | Apply only learnings associated with the repository under review.                        |
| `global` | Apply all organization learnings to every review.                                        |
| `auto`   | Keep public-repository learnings local, but share learnings across private repositories. |

The default `auto` behavior can leak an unsuitable private-repository convention into another private repository with a different stack, so the documentation recommends deliberate scope selection.

### Approval

`knowledge_base.learnings.approval_delay` accepts zero through thirty days:

- `0` activates new chat-sourced learnings immediately.
- `1` through `30` creates a pending request that auto-approves after the delay unless an administrator rejects it.
- Pipeline-inferred learnings continue to activate immediately, even when chat-created learnings require approval.

That asymmetry is documented but not justified as a risk decision.

### Maintenance

Administrators can inspect provenance and usage, search related records, edit or delete stale guidance, and export or import CSV data. The page recommends regular review and replacing obsolete rules instead of accumulating contradictions.

## Claims & Evidence

### Explanation helps a preference generalize

The guidance contrasts a bare instruction with one that records the operational reason, arguing that rationale helps apply the preference in related contexts.

**Caveat/confidence:** High as a prompt-design principle; the page provides illustrative examples rather than comparative results.

### Narrow scope prevents convention cross-contamination

The documentation uses repositories with different languages and conventions to show how global memory can apply irrelevant advice.

**Caveat/confidence:** High as a foreseeable failure mode; actual leakage frequency is not reported.

### Review memory must be maintained

Stale or conflicting records are said to produce inconsistent behavior, so the page recommends periodic review, deletion, and update.

**Caveat/confidence:** High as documented vendor experience, though no quantitative failure data is supplied.

## Important Terms

| Term                | Meaning                                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| Learning            | A durable natural-language statement about a review preference.                                        |
| Provenance          | Metadata showing who, where, and when a learning came from.                                            |
| Scope               | The repository or organization boundary within which a learning applies.                               |
| Approval delay      | A period during which an administrator may reject a chat-created learning before automatic activation. |
| Usage count         | The number of review or chat contexts in which a learning was applied.                                 |
| Cross-contamination | Applying a convention from one context to an incompatible repository or stack.                         |

## Lessons To Reuse

- Promote recurring rules, not temporary exceptions, into durable memory.
- Store rationale and original context alongside the normalized instruction.
- Default to the narrowest useful scope and make broader sharing explicit.
- Expose provenance, activation state, usage, editing, deletion, and export.
- Give learned policy a review and retirement lifecycle.
- Define deterministic precedence and conflict behavior instead of relying on prompt salience.
- Apply equivalent approval expectations to inferred and user-taught memory unless a documented risk analysis justifies the difference.

## Questions for Review

- Why should a learning include the reason for a preference?
  - The reason helps apply it to similar cases without overgeneralizing the literal wording.
- When should feedback remain a one-off decision?
  - When it reflects a temporary exception rather than a recurring team convention.
- How does local scope help?
  - It prevents conventions from one repository from affecting incompatible repositories.
- What does the approval delay govern?
  - New chat-sourced learnings, not pipeline-inferred records or existing learnings.
- Why do learned records require maintenance?
  - Team practices change, and stale or contradictory instructions can produce inconsistent review behavior.

## Connections

- **Related ideas:** Organizational memory, policy-as-code, scoped retrieval, provenance, retention controls
- **Related sources:** Repository custom instructions, version-controlled plans, and model-context selection research
- **Contradictions or tensions:** Learned natural language is easy to create but less deterministic and auditable than version-controlled formal policy.
- **Useful applications:** Review personalization, suppression of recurring false positives, and organization-specific standards

## Open Questions

- How are conflicts resolved deterministically among learnings and higher-priority instructions?
- Why do pipeline-inferred learnings bypass the optional approval queue?
- Is complete version history and rollback available for edited or deleted records?
- How is sensitive review content protected in storage, export, and cross-repository retrieval?
- What evidence shows that a learning was considered, applied, or caused a future comment?

## Takeaways

- Review preferences are durable policy and need scope, provenance, approval, and retirement.
- Rationale makes learned guidance safer to reuse than a context-free command.
- Easy memory creation without conflict handling and maintenance eventually creates inconsistent behavior.
