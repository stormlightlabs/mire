---
title: Walkthrough design
description: See what Mire takes from Greptile, hunk, and CodeRabbit, and the walkthrough fields it proposes.
section: Concepts
group: Concepts
order: 8
---

A walkthrough explains a change in the order a reader should take it. An agent
writes it. Mire stores it in the review file, re-anchors it on refresh, and
renders it. Mire does not call a model, fetch pull requests, or index the
repository.

This page records what three review tools do, what Mire takes from each, and
the fields the walkthrough model will implement. Sources were read on
2026-10-25.

## What the tools do

[Greptile](https://www.greptile.com/docs/code-review/first-pr-review) posts one
summary comment and inline comments. The summary has a plain-language
description, a 0 to 5 confidence score, a list of findings that link to their
inline comments, a files-changed section, and diagrams. Diagram type follows the
change: sequence for multi-service flows, entity relation for schema changes,
class for hierarchies, and flow for control flow. Trivial changes get none.
Inline comments carry P0, P1, or P2 severity and a logic, syntax, or style type.
[Config](https://www.greptile.com/docs/code-review/greptile-config-reference)
toggles each summary section and whether it starts collapsed.

[hunk](https://github.com/modem-dev/hunk) is a terminal diff viewer from
modem-dev for reviewing agent-written code. An agent adds notes to a running
session with `hunk session comment`, or the reviewer loads a JSON
[agent-context file](https://hunk.dev/docs/agents/agent-context-and-stml/) at
launch. That file has an optional `summary` and a `files` list. Each file entry
holds `annotations` with a required `summary`, an optional `rationale`, and a
`newRange` or `oldRange` line range. Files named in the file appear first, in
file order. Notes render beside the lines they explain, `{` and `}` move between
annotated hunks, and `--focus` moves the reviewer to a new note
([comments and annotations](https://hunk.dev/docs/agents/comments-and-annotations/)).
Notes live only in the open window.

CodeRabbit has two surfaces. The
[walkthrough](https://docs.coderabbit.ai/pr-reviews/walkthroughs) comment groups
related files into rows with a description each, and adds sequence diagrams, an
effort estimate, merge risk, related issues and pull requests, suggested labels
and reviewers, and an optional poem.
[Change Stack](https://docs.coderabbit.ai/pr-reviews/change-stack) reorders the
pull request into numbered layers. A layer holds a contract, its implementation,
and its tests together, so it can span directories. Layers follow the order
someone would explain the change, with foundations before what depends on them.
A dependency between layers is recorded, not inferred from numbering. Summaries
and findings attach to line ranges inside a layer. Diagrams appear per layer
only where a layer has something worth drawing. The reader starts on an
overview, moves with J and K between layers, and marks files viewed
([navigation](https://docs.coderabbit.ai/change-stack/navigation.md)). A later
run can regroup the change. CodeRabbit is "asked to keep titles and ordering
that still hold, not guaranteed to."
[Findings](https://docs.coderabbit.ai/change-stack/findings.md) carry separate
type, severity, category, and effort labels, and an attention queue orders them
by what blocks a merge.

## Decisions

### Greptile

| Decision | Item                                                                                             |
| -------- | ------------------------------------------------------------------------------------------------ |
| Adopt    | A short overview at the top, written for someone who has not seen the change.                    |
| Adopt    | A diagram only when the change has a flow or schema to draw, and none for small changes.         |
| Adapt    | Findings linked from the summary become Mire notes shown on the step whose anchors they overlap. |
| Adapt    | P0 to P2 severity maps to Mire's existing severity and annotation intent.                        |
| Skip     | The confidence score. Mire's readiness summary counts open findings and unsafe anchors instead.  |
| Skip     | Re-review triggers, status reactions, and suggested fixes. They need a bot or a model.           |

### hunk

| Decision | Item                                                                                                |
| -------- | --------------------------------------------------------------------------------------------------- |
| Adopt    | A short headline per item with a longer optional body (`summary` and `rationale` in hunk).          |
| Adopt    | Agent order sets file order: files a walkthrough covers come first, in step order.                  |
| Adopt    | Narrative renders beside the anchored lines, with keys to jump between steps.                       |
| Adapt    | Notes that vanish with the window become durable steps that re-anchor on refresh.                   |
| Adapt    | hunk attaches a note without a range to the first hunk. Mire rejects a step without an anchor.      |
| Skip     | STML markup. Narrative is Markdown. Neither the TUI nor the browser renders it yet.                 |
| Skip     | Hiding agent notes by default. A walkthrough is opened on purpose, so its steps show when it opens. |

### CodeRabbit

| Decision | Item                                                                                                                                |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Adopt    | Steps ordered as the change would be explained, not by path. Mire keeps the agent's order.                                          |
| Adopt    | A step groups by meaning across files: contract, implementation, and tests in one step.                                             |
| Adopt    | Each step anchors to line ranges, and its narrative sits next to them.                                                              |
| Adopt    | Dependencies between steps are recorded explicitly.                                                                                 |
| Adopt    | Overview first, then next and previous step keys.                                                                                   |
| Adapt    | Regrouping on each run becomes re-anchoring: steps keep their order and report exact, moved, stale, or ambiguous.                   |
| Adapt    | Per-layer diagrams become an optional Mermaid source on a step, written by the agent.                                               |
| Adapt    | Viewed marks live in the review file, per reader; see [Checkpoints and viewed state](/docs/concepts/checkpoints-and-viewed-state/). |
| Skip     | Cohorts. One ordered list of steps comes first. Parallel groups wait for a real need.                                               |
| Skip     | Effort estimate, merge risk, related issues and pull requests, labels, reviewers, chat, and the poem.                               |
| Skip     | Separate type, category, and effort labels on findings. Mire keeps severity and annotation intent.                                  |

## Proposed fields

A review holds at most one walkthrough. It sits beside the notes and shares the
review revision, so writes are atomic and need the expected revision.

```text
Walkthrough  overview, steps, author, provenance
Step         id, title, narrative, anchors, depends_on, diagram
```

| Field        | Type             | Rule                                                                     |
| ------------ | ---------------- | ------------------------------------------------------------------------ |
| `overview`   | Markdown         | Optional. What the change does and why, read before the first step.      |
| `steps`      | ordered list     | Required, one or more. List order is reading order.                      |
| `author`     | string           | Who wrote the walkthrough, as on notes.                                  |
| `provenance` | producer record  | The same producer provenance notes carry.                                |
| `id`         | string           | Unique within the walkthrough. Mire generates one if the agent omits it. |
| `title`      | string           | Required, one line.                                                      |
| `narrative`  | Markdown         | Required. Why this part changed and what to check.                       |
| `anchors`    | list of anchors  | Required, one or more. The note anchor type: path, side, and line range. |
| `depends_on` | list of step ids | Optional. Each id must name an earlier step with an agent-supplied id.   |
| `diagram`    | Mermaid source   | Optional. Drawn only when the step has a flow or schema worth showing.   |

Mire generates ids only when it stores the write, so an agent cannot name a
generated id in `depends_on` in the same write. A step that another step
depends on needs an id from the agent. Each write replaces the whole
walkthrough. Supplied ids are kept as written, and omitted ids are generated
again.

The model sets explicit bounds, as notes do, and rejects a write that exceeds
one. The proposed values mirror the note limits:

| Bound                       | Proposed value | Mirrors               |
| --------------------------- | -------------- | --------------------- |
| Steps per walkthrough       | 10,000         | `MAX_REVIEW_NOTES`    |
| Anchors across all steps    | 10,000         | `MAX_REVIEW_NOTES`    |
| `overview` bytes            | 1 MiB          | `MAX_NOTE_BODY_BYTES` |
| `narrative` bytes, per step | 1 MiB          | `MAX_NOTE_BODY_BYTES` |
| `diagram` bytes, per step   | 1 MiB          | `MAX_NOTE_BODY_BYTES` |

Ordering follows these rules:

- Mire never reorders steps. The agent chooses the order and Mire stores it.
- `depends_on` may point only backward, so the list has no cycles and reading
  in order never meets a step before its dependency.
- Hunks no step covers are listed after the last step, so the reader can see
  what the walkthrough left out. Mire computes this from the anchors. A hunk
  counts as covered when any step anchor overlaps it, even by one line. Only
  exact and moved anchors count; stale and ambiguous anchors cover nothing.
- Binary, rename-only, and mode-only files have no hunks, so no anchor can
  point at them. They are listed after the uncovered hunks as files no step
  covers.
- In walkthrough mode the file list puts covered files first, in the order of
  the first step that touches each.

Findings stay notes. A step shows the notes whose anchors overlap its anchors,
so one finding is never written twice. A refresh re-anchors step anchors with
the note rules, and a step with a stale or ambiguous anchor says so.
