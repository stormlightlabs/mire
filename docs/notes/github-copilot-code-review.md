---
title: "Using GitHub Copilot Code Review"
sources:
  - https://docs.github.com/copilot/using-github-copilot/code-review/using-copilot-code-review
  - https://docs.github.com/en/copilot/how-tos/use-copilot-agents/request-a-code-review/use-code-review
author: GitHub Docs
captured: 2026-07-14
tags:
  - ai-code-review
  - pull-requests
  - review-authority
  - repository-policy
---

## Summary

GitHub Copilot code review supplies non-blocking, human-triaged review comments across pull requests and local editor changes, with repository policy and optional tools as context but no merge-approval authority.

## Source Boundary

- **Original URL:** The address cited by the source discussion and redirected by GitHub.
- **Current canonical documentation:** The active product how-to describing behavior across GitHub and supported editors.

This documentation establishes product behavior and workflow, not the accuracy, completeness, or efficacy of Copilot's findings.

## Key Ideas

- **Advice and authority are separate:** Copilot always submits a `COMMENT` review, never `APPROVE` or `REQUEST_CHANGES`.
- **Humans control mutations, but effects vary by surface:** Editor-applied suggestions remain uncommitted; accepting suggestions on GitHub.com creates a commit, while “Fix with Copilot” can create a commit or a new pull request.
- **Policy has provenance:** Pull-request review uses repository instructions from the base branch.
- **Re-review is a distinct event:** New commits do not necessarily imply a useful, deduplicated re-evaluation unless automation is configured.
- **Context use can be audited:** When preview MCP and agent-skill support is enabled, session logs show which tools were invoked.
- **Conversation is asymmetric:** Humans can reply to review comments, but the documentation says Copilot does not process those replies as an ongoing dialogue.

## What Copilot Code Review Does

- Reviews pull requests when requested and can be configured through repository rulesets
- Reviews selected code or staged and unstaged changes in supported editors
- Leaves inline comments that humans can react to, discuss, resolve, or hide
- Provides one-click suggested changes where possible
- Accepts repository-wide, root-level, and path-specific instructions
- Can use configured agent skills and MCP servers on GitHub while that support remains in public preview
- Allows feedback on individual comments

## How It Works

### Pull-Request Lifecycle

1. A user or rule requests Copilot as a reviewer.
2. Copilot examines the change using its agentic review capabilities.
3. It submits a non-blocking comment review containing zero or more inline findings.
4. Humans inspect, discuss, resolve, hide, or act on the findings.
5. A re-review can be requested after changes; automatic review of new pushes requires configuration.

Because the review never approves or requests changes, it does not satisfy required approvals and cannot itself block a merge.

### Policy Context

The documented instruction surfaces are:

- `.github/copilot-instructions.md` for repository-wide guidance
- root `AGENTS.md` for repository context and development expectations
- `.github/instructions/**/*.instructions.md` for path-specific guidance

For a pull request, instructions come from the base branch. This prevents proposed changes from silently redefining the policy used to review themselves, but also means newly proposed policy is not active until merged.

### Tools and Traceability

Repository agent skills and MCP servers can provide additional review context. This support is marked public preview and subject to change. Linked session logs allow inspection of invoked tools, which provides better provenance than an unexplained claim of repository awareness.

### Suggested Changes and Mutations

The user explicitly chooses whether to act on suggested edits, but the resulting mutation depends on the interface. In supported editors, applying a suggestion changes local files without committing them. On GitHub.com, accepting a suggested change creates a commit; “Fix with Copilot” may instead create a commit or open a new pull request. A trustworthy interface must state the effect before acceptance rather than treating every “apply” action as equivalent.

## Claims & Evidence

### Automated advice should not count as merge approval

The product enforces this separation by always using a comment review state.

**Caveat/confidence:** High as documented GitHub behavior; it is a product-policy choice, not evidence that every system must use the same authority model.

### Version-controlled instructions can customize review

The reviewer consumes repository-wide, root, and path-scoped guidance from the base branch.

**Caveat/confidence:** High for the documented input mechanism. The page does not measure instruction adherence or conflict resolution.

### Tool logs make context use inspectable

The documentation directs users to session logs to see which MCP servers and tools were called.

**Caveat/confidence:** Medium-high; the integration is preview-only, and invocation does not prove that context was interpreted correctly.

## Important Terms

| Term               | Meaning                                                                              |
| ------------------ | ------------------------------------------------------------------------------------ |
| Comment review     | A submitted review that provides feedback but neither approves nor requests changes. |
| Required approval  | A human or authorized review state required by branch protection before merge.       |
| Suggested change   | A concrete edit attached to a review comment that the user may apply.                |
| Base branch policy | Instructions read from the branch that would receive the pull request.               |
| Re-review          | A new review run after the change set has evolved.                                   |
| Session log        | A record of tool and context activity used during an agentic review.                 |

## Lessons To Reuse

- Keep automated findings separate from merge authority.
- Make every proposed mutation explicit, state whether it edits local files, creates a commit, or opens a pull request, and preserve a human inspection point.
- Record the exact policy revision that governed a review.
- Treat re-review as a revision-aware workflow with stable finding identity and deduplication.
- Expose which tools and external contexts were used.
- Model human replies explicitly if interactive follow-up is a product requirement.
- Keep preview capabilities behind clear compatibility and failure boundaries.

## Questions for Review

- Why can a Copilot review not satisfy a required approval?
  - It always submits a comment review, not an approval or request-changes state.
- Which policy revision governs pull-request review?
  - The custom instructions in the base branch.
- What happens when a user applies a suggested change?
  - It depends on the surface: editor changes remain uncommitted, while acceptance on GitHub.com creates a commit; other fix flows may create a pull request.
- How can a user inspect MCP context use?
  - Open the linked review session and examine its tool-call logs.
- Why can repeated review create noise?
  - A new run may repeat resolved or rejected findings unless identity and history are tracked.

## Connections

- **Related ideas:** Human authority, base-policy integrity, review rounds, comment lifecycle, context provenance
- **Related sources:** GitHub review REST APIs, durable review preferences, and automated-review benchmarks
- **Contradictions or tensions:** Inline comments fit existing workflows, but they are a weak container for investigation history, execution evidence, and cross-round state.
- **Useful applications:** Review bots, editor review, branch-protection design, and agent-tool auditing

## Open Questions

- How should an interactive reviewer ingest and act on human replies safely?
- What stable identity should connect equivalent findings across re-reviews?
- How are conflicting repository-wide and path-specific instructions resolved?
- What review context is retained beyond tool invocation logs?
- What accuracy and triage metrics should determine whether automatic review remains useful?

## Takeaways

- Machine review can advise without becoming an approval authority.
- Policy version, context provenance, and user-controlled edits are part of trustworthy review UX.
- Re-review needs explicit state management to avoid repetitive comment churn.
