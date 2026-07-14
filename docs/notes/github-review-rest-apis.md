---
title: "GitHub REST APIs for Commit Comparison and Pull-Request Reviews"
sources:
  - https://docs.github.com/en/rest/commits/commits
  - https://docs.github.com/rest/pulls/reviews
  - https://docs.github.com/en/rest/commits/commits?apiVersion=2026-03-10
  - https://docs.github.com/en/rest/pulls/reviews?apiVersion=2026-03-10
  - https://docs.github.com/en/rest/pulls/comments?apiVersion=2026-03-10
author: GitHub Docs
captured: 2026-07-14
tags:
  - github-rest-api
  - pull-requests
  - diff-anchors
  - interoperability
---

## Summary

GitHub's REST API can compare remote revisions and publish a stateful pull-request review containing inline comments, but pagination limits, revision binding, anchor semantics, permissions, and notification behavior are correctness constraints rather than incidental API details.

## Source Boundary

- **Cited commit endpoint:** Supplies remote commit comparison and changed-file data.
- **Cited review endpoint:** Redirects to the current grouped review-container and lifecycle reference.
- **Pull-request review-comment endpoints:** Define individual comment operations and current line-range anchoring in more detail.

The versioned sources pin the API behavior described here to GitHub REST API version `2026-03-10`; that version is not the page's publication date.

These APIs model a GitHub-hosted review workflow. They do not replace a complete local Git comparison or a richer internal investigation model.

## Key Ideas

- **Comparison and publication are separate:** Obtaining a remote diff and creating review comments are different API capabilities with different limits.
- **A review is a container:** It groups inline comments with an optional summary and an overall state.
- **Anchors are revision-relative:** A path and line are meaningful only with the side, diff, and commit to which they refer.
- **Pending is a real lifecycle state:** Draft comments can be accumulated before a review is submitted.
- **Truncation is a correctness condition:** Commit and file caps can make an apparently successful response incomplete.
- **Remote APIs need an adapter boundary:** Hosted semantics, permissions, rate limits, and schema changes should not leak into a core review model.

## How Commit Comparison Works

### Revision Selection

The compare endpoint accepts branches, tags, commit identifiers, and qualified fork references within one repository network. Its `BASE...HEAD` route selects commits analogous to `git log BASE..HEAD`, but the API returns commits in chronological order.

### Response

JSON can include:

- base, merge-base, and ahead/behind relationship data;
- ordered commits;
- changed files with status, rename origin, additions, deletions, and totals;
- patch text when available.

Diff and patch media types are also available. Binary changes do not include textual patch content.

### Limits and Failure

- An unpaginated response includes at most 250 commits.
- With pagination, changed files appear only on the first page.
- Changed-file data is capped at 300 files.
- Public data may be read without authentication; private access needs repository Contents read permission.
- Documented failures include not found and server-unavailable responses.

A consumer that needs an exhaustive comparison must detect these boundaries and fall back to another source, commonly local Git.

## How Pull-Request Reviews Work

### Review Lifecycle

A review combines zero or more inline comments, an optional body, and a state:

- Omitting the submission event creates a `PENDING` review.
- Submission uses `APPROVE`, `REQUEST_CHANGES`, or `COMMENT`.
- Pending reviews have no submission timestamp and may be deleted.
- Submitted reviews cannot be deleted; an authorized actor may dismiss them.

Creating reviews and individual review comments can send notifications and trigger secondary rate limiting, so those operations should not be treated as retry-safe local writes without idempotency controls. The cited submission endpoint does not make that same notification or secondary-limit statement.

Creating and submitting reviews requires pull-request write permission. Dismissing a review on a protected branch additionally requires repository-administrator access or a repository role granted dismissal authority.

### Revision Binding

A review can name `commit_id`; if omitted, GitHub binds it to the latest pull-request commit at creation. A review against an older revision can become outdated as the pull request changes.

### Inline Anchors

Current line or range anchors use:

- `path`;
- `line` and `side`;
- optional `start_line` and `start_side` for a range.

File-level comments use `subject_type: file` and do not require `line`. The legacy `position` field counts through diff hunks rather than using a file line and is being retired. Even current line anchors need the original commit and diff context to be relocated reliably after edits.

## Claims & Evidence

### Remote revision comparison is bounded

The documented response caps commits and changed files, and paginated file data is present only on page one.

**Caveat/confidence:** High for the named API version. Limits can change, so clients should bind behavior to a version and test truncation explicitly.

### A pull-request review has a lifecycle distinct from its comments

Pending reviews can collect comments before submission; submitted reviews carry an overall event and different mutation rules.

**Caveat/confidence:** High. Individual review-comment endpoints still matter for later discussion and operations.

### Line numbers alone are insufficient stable identity

Comments bind to a path, side, and revision, while older positions are diff offsets.

**Caveat/confidence:** High as an API-design fact; any relocation algorithm beyond GitHub's own behavior requires additional context and heuristics.

## Important Terms

| Term                 | Meaning                                                                         |
| -------------------- | ------------------------------------------------------------------------------- |
| Merge base           | The common ancestor used to reason about the change between base and head.      |
| Pending review       | A draft review that can accumulate comments before submission.                  |
| Review event         | `APPROVE`, `REQUEST_CHANGES`, or `COMMENT` when submitting.                     |
| Review comment       | Feedback tied to a location in the pull-request diff.                           |
| Side                 | Whether a line belongs to the base (`LEFT`) or head (`RIGHT`) side of the diff. |
| Outdated             | A review or comment whose bound code has changed in a later revision.           |
| Secondary rate limit | Abuse-protection throttling beyond ordinary request quotas.                     |

## Lessons To Reuse

- Keep hosted comparison and publication behind explicit adapters.
- Detect file and commit truncation before claiming complete review coverage.
- Retain base, head, merge base, commit, path, side, hunk, and original/current line data.
- Model pending, submitted, dismissed, and outdated states directly.
- Separate a finding's stable identity from any one forge comment identifier or line number.
- Make publication idempotent and notification-aware.
- Preserve the API version and raw remote identifiers for audit and replay.
- Use a local comparison when the remote representation is incomplete or unavailable.

## Questions for Review

- What does a GitHub review contain?
  - Inline comments plus an optional summary body and an overall review state.
- Why is legacy `position` fragile?
  - It is an offset through diff hunks, not a stable file line.
- What can prevent exhaustive remote comparison?
  - Commit pagination and the 300-file changed-file cap.
- Why bind a review to an explicit commit?
  - So its comments and conclusions retain a precise revision boundary.
- What is special about a pending review?
  - It can accumulate comments and be deleted before submission.

## Connections

- **Related ideas:** Adapter architecture, revision identity, stable anchors, idempotent publication, review rounds
- **Related sources:** Copilot's non-blocking review authority, local Git patching, and SARIF result locations
- **Contradictions or tensions:** GitHub's review model is excellent for collaboration but too narrow for complete investigation history, execution artifacts, and model disagreement.
- **Useful applications:** Forge import/export, review publishing, synchronization, and remote comparison fallback

## Open Questions

- What local fallback should be used when comparison responses are truncated?
- How should a finding be relocated after rebases or rewritten commits?
- How should a richer internal state map to GitHub's smaller review-state model without losing information?
- What idempotency key prevents duplicated comments after uncertain publication failures?
- How should review replies, resolved threads, and dismissals synchronize across repeated runs?

## Takeaways

- Remote comparison limits must be surfaced as completeness failures, not silently ignored.
- Review state, comment state, and code revision are separate dimensions.
- Stable internal findings should outlive any one forge anchor or comment object.
