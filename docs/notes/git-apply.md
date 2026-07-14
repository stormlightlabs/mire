---
title: "git apply"
sources:
  - https://git-scm.com/docs/git-apply
author: Git Project
date: 2026-06-29
captured: 2026-07-14
tags:
  - git
  - patches
  - validation
  - change-safety
---

## Summary

`git apply` validates or applies patch text to files and optionally the index without creating a commit, with atomic failure by default and explicit modes for preflight, partial application, or three-way fallback.

## Source Boundary

- **Git 2.55.0 manual:** Defines command behavior and options as of its June 29, 2026 update.
- **Related Git commands:** `git am`, `git diff`, and worktree operations have distinct responsibilities and are only referenced where the manual distinguishes them.

## Key Ideas

- **Application is not a commit:** The command changes the working tree and/or index but does not create repository history.
- **Preflight is non-mutating:** `--check` reports whether a patch is applicable without applying it.
- **Failure is atomic by default:** If any hunk cannot apply, the whole patch is rejected and the working tree remains untouched.
- **Riskier behavior is explicit:** `--reject`, `--3way`, whitespace fixing, and unsafe paths each alter failure or safety semantics.
- **Applicability depends on state:** Context lines, blob identities, the index, file modes, submodules, path mapping, and whitespace policy can all affect success.

## What `git apply` Does

- Reads one or more patches from files or standard input
- Reports patch statistics or summaries without applying
- Checks applicability against the working tree and/or index
- Applies to the working tree, both tree and index, or only the index
- Attempts a three-way merge when the patch records suitable blob identities
- Filters paths with include/exclude rules or relocates their root
- Detects, warns about, fixes, or rejects configured whitespace errors
- Reverses a patch when requested

## How It Works

### Default Application

Without index options, `git apply` modifies files in the working tree and can operate outside a Git repository. It does not stage or commit the change. When one hunk fails, the default is to apply nothing.

When invoked from a subdirectory inside a repository, patched paths outside that subdirectory are ignored. Callers that require complete application should therefore run from a deliberate root and verify the resulting diff.

### Safe Preflight

`git apply --check <patch>` evaluates whether the patch would apply and detects errors without mutating files. Adding `--index` checks both index and working tree; adding `--cached` checks only index entries.

### Index Semantics

`--index` requires relevant index entries and working-tree copies—including metadata such as file mode—to match before the operation. A patch can therefore be independently applicable to each but still fail the combined consistency requirement.

### Three-Way Fallback

`--3way` can merge when the patch contains original blob identities and those blobs exist locally. It may leave conflict markers, normally implies index use, and is incompatible with `--reject`.

### Partial Application

`--reject` abandons atomic behavior: applicable hunks are written while rejected hunks become `.rej` files. This mode requires explicit cleanup and verification because the working tree can contain only part of the intended change.

### Path and Context Safety

Paths outside the working area are rejected by default. The manual discourages zero-context patches because contextual matching is a safety measure. Path stripping, directory prefixes, inclusion order, and unusual path quoting affect what is targeted.

## Claims & Evidence

### Applicability can be checked before mutation

The manual defines `--check` as a mode that detects applicability errors and turns off application.

**Caveat/confidence:** High. Applicability only shows that hunks fit the selected state; it does not establish semantic correctness, build success, or test success.

### Default patch application is atomic

The documented default rejects the whole patch when some hunks do not apply.

**Caveat/confidence:** High. Atomicity applies to hunk application, not to later formatting, testing, or other commands in a larger workflow.

### Three-way fallback needs original object identity

The command requires blob identities recorded in the patch and those blobs available locally.

**Caveat/confidence:** High. Success can still produce conflicts that need human resolution.

## Important Terms

| Term            | Meaning                                                                                      |
| --------------- | -------------------------------------------------------------------------------------------- |
| Patch           | Diff-formatted instructions for changing file content or metadata.                           |
| Hunk            | One localized region of a patch with surrounding context.                                    |
| Index           | Git's staging area.                                                                          |
| Three-way merge | A merge using original, current, and proposed content rather than contextual patching alone. |
| Reject file     | A `.rej` file containing a hunk that failed during partial application.                      |
| Blob identity   | The object identifier for file content used as a merge ancestor.                             |

## Lessons To Reuse

- Preflight a proposed mutation before applying it.
- Keep all-or-nothing behavior as the default.
- Make partial application and conflict-producing fallback explicit user choices.
- Separate patch application, formatting, testing, staging, and committing into observable steps.
- Preserve the Git version, exit status, standard error, target state, and patch hash for reproducibility.
- Re-inspect the resulting diff even after clean application; syntactic applicability is not correctness.
- Use an isolated or disposable working area when patches or commands are untrusted.

## Questions for Review

- How can a patch be checked without changing files?
  - Run `git apply --check` against the intended working-tree or index state.
- What happens by default when one hunk fails?
  - Nothing is applied.
- What changes when `--reject` is used?
  - Applicable hunks are written and failed hunks are saved as `.rej` files.
- What enables `--3way`?
  - Embedded original blob identities and the corresponding local blobs.
- Does a successful application create a commit?
  - No; committing is a separate operation.

## Connections

- **Related ideas:** Transactional mutation, dry runs, disposable workspaces, provenance, patch verification
- **Related sources:** Git revision comparison and structured analysis fixes
- **Contradictions or tensions:** Three-way merge increases the chance of applying intent but can replace a clean failure with a conflicted intermediate state.
- **Useful applications:** Patch queues, automated remediation, code-review suggestions, and migration tooling

## Open Questions

- How should a higher-level tool protect or account for dirty working-tree state?
- Which diagnostics remain stable across Git versions, locales, and platforms?
- What isolation boundary is appropriate for patches from an untrusted model or source?
- How should submodule, binary, rename, and mode-only changes be verified semantically?

## Takeaways

- A clean preflight is necessary but not sufficient evidence that a patch is correct.
- Atomic application should remain the default; partial and conflicting outcomes need explicit handling.
- Applying, validating behavior, and committing are separate responsibilities.
