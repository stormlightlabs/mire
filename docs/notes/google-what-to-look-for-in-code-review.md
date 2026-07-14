---
title: "What to Look for in a Code Review"
sources:
  - https://google.github.io/eng-practices/review/reviewer/looking-for.html
author: Google Engineering Practices
captured: 2026-07-14
tags:
  - code-review
  - code-health
  - engineering-practice
---

## Summary

A useful code review evaluates the change's design, behavior, complexity, tests, clarity, documentation, and fit with the surrounding system—not merely whether the edited lines compile.

## Source Boundary

- **This page:** A prescriptive checklist for reviewers, nested within Google's broader engineering-practices guide.
- **Linked review standard and navigation pages:** Supply related policy and workflow context, but were not treated as evidence for claims summarized here.

## Key Ideas

- **Design comes first:** Review whether the parts interact coherently, the change belongs in the codebase, and the timing and abstraction are appropriate.
- **Functionality includes user impact:** Check intended behavior, edge cases, concurrency, and the experience of both end users and future developers.
- **Complexity is a defect risk:** Reject unnecessary generality and code that cannot be understood quickly; solve demonstrated needs rather than speculative ones.
- **Tests require review too:** Appropriate tests should normally accompany production changes, and reviewers must check that their assertions are meaningful and would fail for broken behavior.
- **Context changes interpretation:** Read beyond the displayed hunk when necessary and judge whether the change improves or degrades system-wide code health.

## What a Reviewer Examines

- Overall design and integration with the existing system
- Intended functionality, edge cases, UI behavior, and concurrency hazards
- Complexity at line, function, class, and system levels
- Unit, integration, or end-to-end tests appropriate to the change
- Naming, explanatory comments, style, and local consistency
- Documentation affected by build, test, use, release, deletion, or deprecation changes
- Every assigned human-written line, with explicit exceptions when review responsibility is divided
- Positive practices worth reinforcing, not only defects

## How It Works

### Layered Review

The guide moves from system-level questions—design and user-facing behavior—toward implementation qualities such as complexity, tests, names, comments, style, and documentation.

**Synthesis:** This ordering can help prevent surface polish from substituting for behavioral and architectural scrutiny.

### Coverage and Context

The default expectation is to understand every assigned line, while varying scrutiny according to risk. Reviewers should expand from a diff hunk to the enclosing file or wider system whenever the local view is insufficient. When responsibility is intentionally narrower, the reviewer should state what was and was not reviewed.

### Expertise Escalation

A reviewer who lacks the necessary expertise for a specialist area—such as security, privacy, concurrency, accessibility, or internationalization—should ensure that a qualified reviewer covers it rather than silently accepting the gap.

### Feedback Calibration

Mandatory requirements should be separated from personal preferences. The guide recommends marking non-blocking style suggestions as nits and also acknowledging good work for its mentoring value.

## Claims & Evidence

### Code review must cover more than local correctness

The checklist repeatedly connects a change to users, architecture, tests, documentation, and overall code health. Its examples show how a locally plausible edit can still introduce races, excessive complexity, stale documentation, or system-wide inconsistency.

**Caveat/confidence:** High confidence that this is Google's stated practice; it is guidance, not a controlled study demonstrating that every checklist item improves outcomes.

### Running tests cannot replace reasoning about concurrency

The guide argues that races and deadlocks may be difficult to expose through execution alone and therefore require careful reasoning by both author and reviewer.

**Caveat/confidence:** High as an engineering principle; the page provides rationale rather than comparative empirical evidence.

### Google makes every assigned human-written line the default review scope

The page establishes full-line review as the normal case and requires explicit communication when the review covers only selected files or concerns.

**Caveat/confidence:** High as a stated process rule; practical coverage can still vary with generated data, reviewer expertise, and change size.

## Important Terms

| Term         | Meaning                                                                                        |
| ------------ | ---------------------------------------------------------------------------------------------- |
| `CL`         | Change list: the proposed set of code changes under review.                                    |
| Code health  | The system's long-term understandability, maintainability, testability, and reliability.       |
| Nit          | A non-blocking suggestion based on preference or minor polish rather than a required standard. |
| Review scope | The files, lines, or specialist concerns a reviewer is responsible for evaluating.             |

## Lessons To Reuse

- Begin with intent, design, and behavior before inspecting stylistic details.
- Treat tests and documentation as first-class parts of a behavioral change.
- Record review coverage and expertise gaps explicitly.
- Expand context deliberately when the diff cannot establish system fit.
- Distinguish required corrections from optional polish.

## Questions for Review

- Why is a diff hunk alone sometimes insufficient for review?
  - The enclosing file or wider system can reveal complexity, contracts, and interactions hidden by the local view.
- What should a reviewer ask about tests beyond whether they pass?
  - Whether they are appropriate, maintainable, make useful assertions, and would fail when the behavior is broken.
- How should a reviewer handle an area outside their expertise?
  - Make the coverage gap explicit and obtain review from someone qualified in that area.
- When should style feedback block a change?
  - When it enforces an applicable standard, not when it is merely a personal preference.

## Connections

- **Related ideas:** Risk-based review, change intent, negative-space review, specialist review, test quality, code-health stewardship
- **Related sources:** Empirical research on review navigation orders and benchmarks of automated review coverage
- **Contradictions or tensions:** Reviewing every line can conflict with speed on very large changes; the guide resolves this through judgment, scoped responsibility, and explicit coverage.
- **Useful applications:** Human review checklists, reviewer training, review policy, and evaluation criteria for automated review systems

## Open Questions

- How should teams measure whether every assigned line was meaningfully understood rather than merely visited?
- Which review dimensions benefit most from specialist reviewers or automated checks?
- How should the checklist adapt to generated code, vendored code, data migrations, or unusually large changes?

## Takeaways

- Review the behavior and design of a change, not only its syntax or edited lines.
- Make coverage, context expansion, and expertise gaps visible.
- Simplicity, meaningful tests, documentation, and code health are review outcomes, not secondary polish.
