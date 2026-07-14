---
title: "Greptile AI Code Review and Runtime Validation"
sources:
  - https://www.greptile.com/what-is-ai-code-review
  - https://www.greptile.com/
  - https://www.greptile.com/trex
author: Greptile Team
captured: 2026-07-14
tags:
  - ai-code-review
  - contextual-retrieval
  - runtime-validation
  - vendor-claims
---

## Summary

Greptile presents AI review as an iterative, repository-aware layer inside the pull-request workflow and extends static reasoning with sandboxed runtime validation that can attach executable evidence to findings.

## Source Boundary

- **AI code-review guide:** Mixes general workflow advice, a buyer's guide, a showcased pull request, and Greptile-specific marketing claims.
- **Product home page:** Describes Greptile's graph index, parallel review agents, learned preferences, integrations, and deployment options at a high level.
- **TREX page:** Describes runtime-validation behavior and evidence artifacts; it is a product page, not independent evaluation.

Metrics, testimonials, and efficacy statements on these pages are vendor claims unless independently substantiated elsewhere.

## Key Ideas

- **The diff is an entry point:** Review should connect edited lines to related functions, dependencies, APIs, configuration, tests, documentation, and history.
- **Review is iterative:** A material pull-request update should trigger another pass because earlier findings may be obsolete and new cross-layer risks may have appeared.
- **Generation and evaluation should be separated:** An independent review role reduces shared assumptions and avoids having a generator grade its own work.
- **Repository policy is context:** Natural-language rules and prior team feedback can shape later reviews.
- **Execution strengthens evidence:** Runtime validation can start services, create or run targeted tests, mock inputs, exercise APIs and UIs, and attach logs or traces to a finding.
- **Noise remains a core limitation:** The guide acknowledges false positives, overbroad advice, uneven context, stochastic output, and data-security concerns.

## What the Described System Does

- Builds a graph of repository files, functions, and dependencies
- Uses parallel agents to inspect changes and effects beyond the diff
- Produces pull-request summaries, inline comments, sequence diagrams, and impact-ranked findings
- Learns review preferences from team comments and accepts explicit custom rules
- Re-runs review when a pull request changes
- Supports follow-up questions and handoff of comment context to coding agents
- Offers self-hosted deployment and use of customer-selected model providers
- Uses TREX to run a pull-request branch in a sandbox and collect runtime evidence

## How It Works

### Contextual Review

1. A pull request triggers review.
2. The diff is related to a repository graph and other relevant artifacts.
3. Parallel review agents inspect changed behavior and cross-layer effects.
4. The system emits orientation material and line-specific findings.
5. Findings are ranked, discussed, or passed to another agent for remediation.
6. A later revision is rescanned rather than assumed equivalent to the first.

The pages do not specify graph construction, retrieval ranking, context limits, prompt structure, deduplication, or confidence calibration.

### Runtime Validation

1. TREX examines the pull request, repository, existing tests, and stack.
2. It chooses services, development servers, mocks, API calls, browser agents, or targeted tests to run.
3. It executes the branch in a sandbox.
4. When a check fails, it relates the failure to relevant code.
5. It attaches evidence such as logs, screenshots, traces, scripts, video, or API output to a pull-request comment.

The public page does not describe sandbox isolation, permissions, network policy, secret handling, timeouts, reproducibility, or cleanup.

### Learning and Rules

The product accepts plain-language standards and says it learns team conventions by reading review comments. The source does not explain which signals become durable rules, how conflicts are resolved, or how users inspect and undo learned behavior.

## Claims & Evidence

### Greptile presents a case study of cross-layer mismatch detection

The guide presents a large public pull request in which review comments connected frontend flags, server defaults, authentication paths, GraphQL, environment configuration, and documentation. It also reports multiple review rounds as the change expanded.

**Caveat/confidence:** Medium. The vendor page reports iterative comments on a linked pull request but cannot establish representative accuracy, causality, or usefulness across repositories.

### Independent review reduces shared assumptions

The buyer's guide argues that creation and evaluation are different jobs and that separation reduces self-review bias and correlated blind spots.

**Caveat/confidence:** Medium-high as a design rationale; the page provides no controlled comparison of independent and shared reviewers.

### Greptile claims TREX can surface runtime-only failures

TREX is described as exercising real services, APIs, mocks, browsers, and tests, with artifacts attached as evidence. The page claims roughly 20% more bugs are found than by review alone.

**Caveat/confidence:** High confidence in the stated intended mechanism; low confidence in the percentage because no dataset, denominator, methodology, or uncertainty is supplied.

### Greptile claims shorter merge times and more defects caught

The guide reports a median merge-time change from about 20 hours to 1.8 hours, while the home page makes broader comparative claims.

**Caveat/confidence:** Low. The pages do not publish sample selection, baselines, confounders, study design, or raw data.

### Repository indexing and configuration mitigate model inconsistency

The source presents a repository graph, custom rules, severity controls, and learned preferences as ways to improve relevance and repeatability.

**Caveat/confidence:** Medium as a plausible mechanism; no repeatability or false-positive measurements are included.

## Important Terms

| Term                  | Meaning                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------- |
| Context-aware review  | Analysis grounded in related repository artifacts rather than only changed lines.                       |
| Beyond-the-diff       | Examination of dependent code, layers, configuration, and operational assumptions outside edited hunks. |
| Graph index           | A representation of files, functions, and dependencies used to retrieve related context.                |
| Iterative pass        | A new review of a changed pull-request revision.                                                        |
| Independent reviewer  | An evaluation role separated from code generation.                                                      |
| Runtime validation    | Executing changed behavior or focused checks to obtain empirical evidence.                              |
| Stochasticity         | Variation between model outputs for equivalent inputs.                                                  |
| Signal-to-noise ratio | The proportion of findings valuable enough to justify developer attention.                              |

## Lessons To Reuse

- Bind every review to a concrete revision and re-evaluate after material changes.
- Retrieve context by the suspected behavior or finding rather than treating repository size as useful by itself.
- Separate summaries for orientation from evidence needed to justify a defect claim.
- Prefer executable artifacts—tests, logs, traces, screenshots, and API output—over unsupported prose.
- Make learned preferences inspectable, scoped, reversible, and distinct from formal policy.
- Measure accepted findings, false positives, escaped defects, and developer time rather than raw comment volume.
- Treat runtime validation as untrusted code execution requiring an explicit isolation and permissions model.
- Keep vendor examples and metrics separate from independently validated evidence.

## Questions for Review

- What makes a review context-aware?
  - It connects the change to related code, dependencies, contracts, configuration, tests, documentation, and history.
- Why should a changed pull request be rescanned?
  - Earlier findings may no longer apply, while new interactions and defects may have been introduced.
- Why separate code generation from review?
  - Independent roles are less likely to share the same assumptions or soften evaluation of their own output.
- What does runtime validation add to a finding?
  - Observable execution evidence and a possible reproduction path, not merely another inference.
- What does a showcased pull request prove?
  - That the product produced comments on that case; it does not establish general accuracy or broad defect prevention.

## Connections

- **Related ideas:** Retrieval-augmented analysis, specialist review passes, evidence ledgers, sandboxed testing, reviewer independence
- **Related sources:** AI review benchmarks, review-navigation research, learned review preferences, and automated verification workflows
- **Contradictions or tensions:** More repository context can improve relevance yet also dilute attention; more comments can increase apparent coverage while reducing trust.
- **Useful applications:** Pull-request review, regression investigation, review-to-fix handoffs, and empirical verification of model findings

## Open Questions

- How are the graph index and finding-specific retrieval implemented and bounded?
- How are severity, confidence, duplicate findings, and repeated-run consistency measured?
- Which review feedback becomes a learned rule, and can users audit or roll it back?
- What security boundary governs runtime execution, network access, secrets, and generated tests?
- How were the merge-time and additional-bug percentages calculated?
- How does the workflow behave for monorepos, multi-repository changes, generated code, and unavailable dependencies?

## Takeaways

- High-value review relates a revision to wider behavioral context and revisits it after changes.
- Runtime evidence can make a finding substantially more actionable than prose alone.
- Vendor-authored workflows can supply design ideas, but their performance claims need independent validation.
