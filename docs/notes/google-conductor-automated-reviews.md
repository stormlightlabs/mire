---
title: "Conductor Update: Introducing Automated Reviews"
sources:
  - https://developers.googleblog.com/conductor-update-introducing-automated-reviews/
  - https://github.com/gemini-cli-extensions/conductor
author: Sherzat Aitbayev, Mahima Shanware, and Jay Kornder
date: 2026-02-13
captured: 2026-07-14
tags:
  - automated-review
  - agentic-development
  - verification
  - specifications
---

## Summary

Conductor adds a post-implementation verification stage that reviews agent-produced code against persistent plans, specifications, project guidelines, tests, and basic security expectations.

## Source Boundary

- **Google Developers Blog announcement:** Describes the intended workflow and product capabilities from the authors' perspective.
- **Linked Conductor extension repository:** Provides the installable implementation, but its code and behavior were not independently audited for this note.

## Key Ideas

- **Persistent context:** Project awareness is moved from ephemeral chat into version-controlled Markdown plans, specifications, and guidelines.
- **Verification closes the loop:** Review occurs after implementation and produces a report intended to guide the next iteration.
- **Plan compliance is reviewable:** The system checks whether implementation addressed the declared phases and requirements, not only whether the code looks plausible.
- **Evidence is aggregated:** Static and logic analysis, test results, coverage information, and basic security checks appear in one review report.
- **Humans retain oversight:** The article frames automation as labor under human architectural direction, not as unsupervised approval.

## What Conductor's Automated Review Does

- Analyzes newly generated files for code-quality and logic problems
- Compares implementation with `plan.md`, `spec.md`, and project guidelines
- Runs relevant unit and integration tests and reports results and coverage
- Checks for selected security risks such as exposed secrets, possible personal-data leaks, unsafe input handling, and injection hazards
- Categorizes findings as high, medium, or low severity
- Identifies affected file paths and lets a developer begin a follow-up work track

## How It Works

### Lifecycle

1. Project intent and constraints are written to persistent Markdown artifacts.
2. A coding agent implements the planned work.
3. Automated Review adds a verification stage after implementation.
4. The system compares the result with the plan, specification, and guidelines.
5. It incorporates code analysis and relevant test execution into a report.
6. The developer uses categorized findings to direct the next iteration.

### Context and State

The described workflow treats version-controlled files as durable shared state across agent sessions. Plans and specifications are therefore both implementation inputs and verification criteria.

### Review Output

The report is organized around actionable, severity-ranked findings and exact paths. The article presents the report as a handoff into another focused work track rather than a terminal approval decision.

## Claims & Evidence

### Durable files make AI-assisted development more context-driven

The article says Conductor externalizes awareness from transient conversations into version-controlled Markdown, allowing planning artifacts to persist across work stages.

**Caveat/confidence:** High confidence that this is the intended design. The article does not compare this workflow experimentally with chat-only development.

### Automated Review can check implementation against declared intent

Plan and specification files are used as criteria for detecting omitted roadmap phases or requirements.

**Caveat/confidence:** Medium. The mechanism is described clearly, but the announcement supplies no benchmark for completeness, false positives, or false negatives.

### Automated Review incorporates test and basic-security signals into its report

The system is described as executing relevant tests and scanning for a bounded set of common security problems before reporting results.

**Caveat/confidence:** Medium. This is a product-capability claim without detail on sandboxing, test selection, supported ecosystems, coverage interpretation, or security-analysis depth.

## Important Terms

| Term                  | Meaning                                                                                  |
| --------------------- | ---------------------------------------------------------------------------------------- |
| Automated Review      | Conductor's post-implementation verification and reporting stage.                        |
| Plan compliance       | Checking whether implementation addressed the work declared in a plan and specification. |
| Guideline enforcement | Comparing changes with project-specific style and development rules.                     |
| Work track            | A follow-up agent workflow started from a reported finding.                              |
| Persistent context    | Version-controlled project knowledge that survives individual chat sessions.             |

## Lessons To Reuse

- Store intent and constraints in durable artifacts that can later serve as review criteria.
- Make verification a named lifecycle stage after implementation.
- Combine code findings with actual test results while keeping their evidentiary strength distinct.
- Route findings into focused follow-up work rather than treating a generated report as self-executing.
- Preserve human responsibility for architectural and risk decisions.

## Questions for Review

- What makes a plan useful for both implementation and review?
  - It records observable requirements and constraints that can be checked against the result.
- Where does Automated Review sit in the described lifecycle?
  - After the coding agent finishes implementation, as a distinct verification step.
- What kinds of evidence appear in the report?
  - Static and logic findings, plan and guideline compliance, test results and coverage, and selected security checks.
- What important reliability information is absent from the announcement?
  - Accuracy measurements, false-positive rates, supported environments, test-selection details, and failure handling.

## Connections

- **Related ideas:** Executable specifications, verification loops, durable agent state, review reports, severity triage
- **Related sources:** General code-review criteria, AI review benchmarks, and interchange formats for analysis findings
- **Contradictions or tensions:** Markdown is accessible and versionable but may be too weakly structured for complete machine state or reproducibility.
- **Useful applications:** Agent workflows, post-generation audits, compliance reports, and cross-session handoffs

## Open Questions

- How does the reviewer determine which tests are relevant, and what happens when the environment cannot run them?
- How precisely are plan requirements mapped to changed code and evidence?
- Are findings stable across repeated reviews, and how are resolved findings tracked?
- What isolation and permission model protects developers when tests or analysis commands execute?

## Takeaways

- Durable plans and specifications can double as verification inputs.
- Post-implementation review should combine intent, code, tests, and guidelines.
- Product announcements establish intended behavior, not independent evidence of review accuracy.
