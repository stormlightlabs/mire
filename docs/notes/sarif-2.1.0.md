---
title: "Static Analysis Results Interchange Format (SARIF) 2.1.0"
sources:
  - https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html
  - https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/sarif-v2.1.0-errata01-os-complete.html
  - https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json
author: Michael C. Fanning and Laurence J. Golding, editors
date: 2023-08-28
captured: 2026-07-14
tags:
  - sarif
  - static-analysis
  - interoperability
  - findings
---

## Summary

SARIF 2.1.0 is a JSON interchange standard for static-analysis runs, tools, artifacts, rules, results, locations, identity across runs, suppressions, and structured fixes, with explicit distinctions between successful emptiness and failed population.

## Source Boundary

- **Latest-stage document:** Convenient entry point for SARIF 2.1.0.
- **Immutable Errata 01 OASIS Standard:** The fixed edition used for normative interpretation here.
- **Normative JSON Schema:** Machine-readable structural constraints.

SARIF standardizes exchange of analysis results. It does not define a full conversational code-review system or require it to be the canonical storage model of a producer.

## Key Ideas

- **A log contains runs:** Each run records one tool execution context and its analysis outputs.
- **Results are rule-based evidence records:** A result can carry rule identity, kind, severity, message, locations, fingerprints, suppression, baseline state, and fixes.
- **Absence and failure differ:** Empty arrays mean successful absence; `null` can indicate that a property could not be populated.
- **Identity survives runs:** Fingerprints and correlation identifiers help match the same logical result over time.
- **Human judgment is representable:** Result kind `review` explicitly indicates that a person must decide whether the condition is acceptable.
- **Fixes are not unified patches:** SARIF fixes describe artifact-region replacements in original coordinates.

## How It Works

### Top-Level Model

A SARIF log is UTF-8 JSON with:

- `version: "2.1.0"`;
- a required `runs` property whose value is `null` or an array containing zero or more runs.

The array is normally nonempty. `runs: []` means no run data was available, while `runs: null` means population was attempted and failed.

Each run can identify the tool and rules, invocation details, artifacts, taxonomies, graphs, results, and provenance relevant to one analysis operation.

### Result Model

A result normally identifies a rule and includes a message. Optional properties describe:

- `kind`, such as pass, fail, open, informational, not applicable, or review;
- `level`, such as error, warning, note, or none;
- one or more physical or logical locations;
- related locations and code flows;
- partial fingerprints or correlation identifiers;
- suppression and baseline state;
- work-item links, properties, attachments, and structured fixes.

Multiple primary locations are appropriate when all of them must change to correct one condition, not simply because a result discusses several relevant places.

### Cross-Run Identity

Rule identity plus fingerprints or correlation identifiers can relate a current result to an earlier one even when line numbers move. Fingerprint design remains the producer's responsibility and must balance stability against accidental collision.

### Baselines

`baselineState` distinguishes new, unchanged, updated, or absent results relative to a baseline. When a producer emits baseline state for a run, coverage should be consistent across that run rather than applied selectively.

### Failure Semantics

The format distinguishes:

- `runs: []`: the log contains no run data;
- `runs: null`: run data could not be populated;
- `results: []`: analysis completed and found no results;
- `results: null`: result population or analysis did not complete.

Invocation success and tool-execution notifications provide additional evidence. Consumers should not infer a clean analysis merely from the absence of findings.

### Fixes

A fix contains artifact changes expressed as replacements of regions in original, unmodified artifact coordinates. The specification also notes that a text fix cannot be applied safely without knowing the target encoding. The format does not itself execute the fix or create a Git commit.

**Consumer-side safety recommendation:** Validate overlap, current content, encoding, and applicability before mutation.

## Claims & Evidence

### Heterogeneous analyzers can share a common result envelope

SARIF defines common structures for tools, rules, locations, messages, artifacts, invocation status, and fixes.

**Caveat/confidence:** High as the purpose of the standard. Semantic quality still depends on producers using rule identities, severities, and locations consistently.

### Result identity can persist across runs

Fingerprints, correlation identifiers, rule identity, and baseline state provide mechanisms for matching logical findings.

**Caveat/confidence:** High for representational support; SARIF does not supply one universally stable fingerprint algorithm.

### Successful “no findings” differs from incomplete analysis

Nullability, invocation status, and notifications let a producer communicate failure separately from an empty successful result set.

**Caveat/confidence:** High. Consumers must preserve these distinctions instead of normalizing all missing values to empty arrays.

## Important Terms

| Term              | Meaning                                                                    |
| ----------------- | -------------------------------------------------------------------------- |
| `sarifLog`        | The top-level SARIF JSON object.                                           |
| Run               | One analysis execution and its tool, inputs, artifacts, and results.       |
| Rule              | The check or condition associated with a result.                           |
| Result            | One reported condition, outcome, or review item.                           |
| Physical location | An artifact URI and optional region identifying source evidence.           |
| Fingerprint       | Producer-computed identity material for correlating a finding across runs. |
| Baseline state    | A result's relationship to an earlier analysis, such as new or unchanged.  |
| Suppression       | Metadata explaining why a result is accepted, hidden, or excluded.         |
| Fix               | One or more artifact-region replacements proposed for a result.            |

## Lessons To Reuse

- Preserve the difference between “completed with no findings” and “could not complete.”
- Give findings stable rule identity and correlation material independent of current line numbers.
- Keep evidence locations, severity, result kind, suppression, and lifecycle state distinct.
- Treat export formats as interoperability boundaries when the internal workflow needs richer discussion, verification, or patch history.
- Validate structured fixes against the exact artifact state before applying them.
- Declare which internal fields are lost or approximated during export and import.
- Use `review` semantics when a condition requires human judgment rather than presenting it as a proven failure.

## Questions for Review

- Why does SARIF distinguish `null` from an empty array?
  - Failure to populate data is different from a successful run with nothing to report.
- How can a finding be matched across runs?
  - Through stable rule identity plus fingerprints or correlation identifiers.
- What does result kind `review` mean?
  - Human judgment is required to decide whether the condition is acceptable.
- What does a SARIF fix contain?
  - Per-artifact replacements tied to regions in the original artifact.
- Why might SARIF be an export rather than the internal model?
  - It models analysis interchange well but not an entire conversational investigation and decision history.

## Connections

- **Related ideas:** Finding schemas, cross-run deduplication, baselines, suppression, structured remediation
- **Related sources:** Forge review anchors, evidence-backed automated review, and patch application
- **Contradictions or tensions:** Interoperability favors a standardized common denominator, while a rich review ledger may need more domain-specific state.
- **Useful applications:** Static-analysis aggregation, CI results, security findings, editor integration, and review export

## Open Questions

- Which internal discussion, model, verification, and decision fields are intentionally lost on export?
- How should confidence map when SARIF separates result kind and level but does not impose one confidence model?
- Which fingerprint strategy remains stable across movement and refactoring without merging distinct findings?
- How should a consumer reconcile conflicting rule identities or taxonomies from multiple producers?
- Which security checks are required before opening artifact URIs or applying fixes from untrusted logs?

## Takeaways

- SARIF is a strong analysis interchange format, not automatically a complete review-session model.
- Finding identity and failure completeness deserve explicit representation.
- Structured fixes still require state validation and a separate safe mutation workflow.
