---
title: "Not One to Rule Them All: Mining Meaningful Code Review Orders From GitHub"
sources:
  - https://arxiv.org/abs/2506.10654
  - https://doi.org/10.1145/3756681.3756961
  - https://github.com/abiUni/github_reviews_study
  - https://github.com/abiUni/mining_cr_data
author: Abir Bouraffa, Carolin E. Brandt, Andy Zaidman, and Walid Maalej
date: 2025-06-12
captured: 2026-07-14
tags:
  - code-review
  - navigation
  - empirical-software-engineering
  - pull-requests
---

## Summary

Reviewers often leave the default alphabetical file order, especially in larger reviews, but no single alternative order explains their behavior; tools should expose several useful cues and leave navigation under reviewer control.

## Source Boundary

- **Conference paper and arXiv copy:** Define the questions, dataset, method, findings, interpretation, and validity threats.
- **Analysis package:** Supplies replication artifacts for the reported study.
- **Modified mining tool:** Supplies the data-collection implementation used by the authors.

Repository artifacts support reproducibility but do not independently validate the paper's conclusions.

## Key Ideas

- **Alphabetical order is only a default:** A substantial minority of eligible reviews contain a return to an earlier alphabetical file.
- **Reviewers use several strategies:** Diff size, similarity to the pull-request description, and test-first navigation each explain subsets of behavior.
- **No universal order emerges:** Alignment with simple candidate orders weakens as reviews cover more files.
- **Large changes are navigation problems:** Reviewers appear to need an overview and multiple entry-point cues rather than a longer flat file list.
- **Observed comments are an imperfect trace:** Comment order does not reveal every file viewed or the mental sequence of review.

## Research Questions

1. How often do reviewers comment in a non-alphabetical file order?
2. How closely do those reviews follow largest-diff-first, description-relevance-first, or test-first orders?
3. Is navigation order associated with the fraction of changed files receiving comments?
4. Is order associated with the first review round's outcome?

## How the Study Works

### Dataset

- 23,241 pull requests from 100 active, popular Java and Python repositories
- 50 repositories sampled per language
- Pull requests changed at least two files and received line-level comments on at least two files
- Only the first review round by the first non-author, non-bot reviewer was reconstructed
- Replies and later reviewers were excluded to reduce social and temporal effects

### Navigation Proxy

Chronological line-comment paths stand in for navigation. A review is classified as non-alphabetical when a later comment targets a file that appears earlier alphabetically—a **step back**.

### Candidate Orders

| Order              | Ranking mechanism                                                                                |
| ------------------ | ------------------------------------------------------------------------------------------------ |
| Largest-diff-first | Added plus deleted lines before the first review round                                           |
| Most-similar-first | CodeT5+ embeddings compare each file's changed lines with the pull-request title and description |
| Test-first         | Paths containing `test` rank before production paths when both kinds receive comments            |

Observed and candidate ranks are compared with Kendall rank correlation. Exact agreement is **strict**; correlation of at least 0.5 is **soft**.

### Coverage and Outcome

File coverage is commented changed files divided by all changed files. First-round GitHub review state supplies the outcome. These measures do not reveal defect detection, time spent, un-commented inspection, or review quality directly.

Coverage analysis excluded 98 pull requests with at least 300 changed files. Outcome analysis omitted 96 pull requests without a valid review status.

## Findings

### Non-alphabetical navigation is common

- 10,377 of 23,241 reviews, or 44.6%, contained at least one step back.
- Once comments covered at least three files, non-alphabetical reviews were the majority.
- Larger reviewed-file counts were associated with later divergence from alphabetical order.

### Alternative orders explain only subsets

| Candidate order    | Strict matches |              Soft matches | Applicable population               |
| ------------------ | -------------: | ------------------------: | ----------------------------------- |
| Largest-diff-first |  2,134 (20.6%) |             2,584 (24.9%) | 10,377 non-alphabetical reviews     |
| Most-similar-first |  1,827 (17.6%) |             2,442 (23.5%) | 10,377 non-alphabetical reviews     |
| Test-first         |  1,188 (29.0%) | Not equivalently reported | 4,059 mixed test/production reviews |

Alignment with the size and semantic-relevance orders approached zero as more files received comments. The authors infer that large reviews likely combine strategies or personal heuristics.

### Coverage and outcome differ slightly

- Mean file coverage was 42.1% for non-alphabetical reviews and 39.0% for alphabetical reviews; the reported comparison was statistically significant.
- Non-alphabetical reviews were underrepresented among approvals relative to their share of the whole sample.

These are associations, not evidence that reordering causes greater coverage or stricter outcomes.

## Claims & Evidence

### A single fixed file order does not represent many real reviews

Nearly 45% of eligible first-round reviews contained at least one chronological return to an earlier alphabetical file, and the share rose as more files received comments.

**Caveat/confidence:** High for this dataset and definition; the step-back rule is sensitive and comment order is only a navigation proxy.

### Simple alternative orders do not explain large-review behavior

Largest-diff-first and most-similar-first lost alignment as reviewed-file count grew. Test-first matched 29% strictly in its applicable subset, while the remaining reviews showed consistently negative correlation with test-first.

**Caveat/confidence:** Medium-high. The candidates are deliberately simple, semantic ranking was not independently validated, and accidental overlap between strategies was not measured.

### Non-alphabetical reviews cover slightly more changed files

The observed mean difference was 3.1 percentage points and statistically significant.

**Caveat/confidence:** Medium. Coverage means “received a comment,” not “was read well,” and uncontrolled factors such as reviewer motivation, familiarity, and change complexity could explain the association.

## Important Terms

| Term               | Meaning                                                                             |
| ------------------ | ----------------------------------------------------------------------------------- |
| Step back          | A later comment targets a file earlier in alphabetical path order.                  |
| Strict order       | Observed and candidate ranks agree exactly.                                         |
| Soft order         | Observed and candidate ranks have Kendall correlation of at least 0.5.              |
| Review coverage    | Commented changed files divided by all changed files.                               |
| Most-similar-first | Files ranked by semantic similarity between their diffs and the change description. |
| Macro structure    | A change overview that helps a reviewer choose an entry point and strategy.         |

## Lessons To Reuse

- Give reviewers an overview before asking them to traverse files.
- Offer multiple transparent cues—risk, diff size, tests, semantic relevance, and dependencies—without making one mandatory.
- Explain why an order is suggested and allow the reviewer to change it cheaply.
- Treat navigation of a large change as an explicit design problem.
- Instrument actual views and transitions when evaluating a review interface; comments alone are an incomplete trace.
- Control for change size, reviewer familiarity, and project norms before attributing outcomes to presentation order.

## Questions for Review

- What is the paper's central empirical result?
  - 44.6% of eligible first-round reviews contained non-alphabetical comment navigation, while no single alternative order explained complex reviews.
- How is non-alphabetical navigation detected?
  - By at least one chronological step back to an earlier alphabetical file.
- Which candidate order had the largest strict share in its applicable subset?
  - Test-first, at 29% of non-alphabetical reviews containing both test and production files.
- Why does the coverage result not prove that reordering improves review?
  - The study is observational and does not control all reviewer and change characteristics.
- What is the most defensible interface implication?
  - Provide overview information, several explainable ordering cues, and reviewer-controlled navigation.

## Connections

- **Related ideas:** Risk-based ordering, test-first reading, semantic relevance, change overviews, reviewer agency
- **Related sources:** General review guidance and evaluations of context selection for automated review
- **Contradictions or tensions:** Guidance to inspect every assigned line does not imply that every reviewer should inspect files in the same order.
- **Useful applications:** Diff viewers, review queues, review telemetry, and experiments on presentation order

## Open Questions

- What sequences do reviewers actually view when direct telemetry is available?
- Which combinations of strategies appear in large reviews?
- Does a flexible order causally improve defect detection, coverage, or review time?
- How do reviewer expertise and repository familiarity affect navigation?
- Can dependency- or execution-flow ordering outperform file-level heuristics?

## Takeaways

- Alphabetical presentation is not a faithful model of many real review paths.
- Large reviews require orientation and choice, not one replacement ordering rule.
- Comment-order mining is useful evidence but cannot establish causation or complete review behavior.
