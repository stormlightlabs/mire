---
name: fab-decompose
description: Cut an idea, a spec or a review finding into tasks a run can take, grouped and ordered, and write them to the repository's tracker. Use for /fab-decompose, or when work is understood well enough to file but has not been filed.
argument-hint: "<idea, spec or finding>"
---

# Decompose

Turn something understood into tasks someone can claim. The input is an idea,
a spec on `main`, or a review finding. This skill decides what to write; the
tracker holds it.

## The primitives

| Primitive  | What it is                                                         |
| ---------- | ------------------------------------------------------------------ |
| Collection | Groups run units. Holds their order and what crosses them.         |
| Run unit   | A body of work one run takes. Holds tasks, and is never claimed.   |
| Task       | One claim, one branch, one pull request.                           |
| Dependency | One task waits for another to land.                                |

The **Tasks** section of `AGENTS.md` maps each primitive to the tracker and
names the command that writes it. Without that section, use GitHub:

| Primitive  | GitHub                    | Write it with                                                    |
| ---------- | ------------------------- | ---------------------------------------------------------------- |
| Collection | a milestone               | `gh api repos/{owner}/{repo}/milestones -f title=… -f description=…` |
| Run unit   | an issue with sub-issues  | `gh issue create --milestone …`                                  |
| Task       | a sub-issue               | `gh issue create`, then `gh api repos/{owner}/{repo}/issues/<unit>/sub_issues -F sub_issue_id=<id>` |
| Dependency | blocked-by                | `gh api repos/{owner}/{repo}/issues/<task>/dependencies/blocked_by -F issue_id=<id>` |

Both endpoints take the issue's numeric `id`, not its number. Read it with
`gh api repos/{owner}/{repo}/issues/<n> --jq .id`.

A lone run unit opens no collection. File it in an existing collection it
belongs to, or leave out the collection flag (`--milestone` above).

## Before writing anything

Look up what is already filed from the same source: search the tracker's
titles and bodies for the idea's, spec's or finding's path or ID. Update or
skip a record that exists rather than filing it again.

Every task needs a stop rule anyone can check without judgment. When you
cannot write one, you have reached the edge of what is understood. If one
decision is missing, take it back to `fab-rubber-duck` for a spec and write
nothing. If the work is understood but the rule is awkward, the cut is wrong:
a task that needs three conditions is usually two tasks.

A task whose result is a recorded decision rather than a code change is fine,
and says so.

## Run units

One per body of work that fits a run. Its body gives the goal, its own stop
rule (more than "every task is done"), the source, and what it must not
absorb. It carries no status: nothing claims it, so a status would only copy
its tasks'.

Two tiers only. A task that needs tasks of its own is a second run unit beside
the first, never nested under it.

## Collections

Write one when the work needs more than one run unit; a collection of one is a
heading. Its description holds what a run cannot see from inside one unit:
the order the units run in, and what crosses them, such as a dependency or a
file two of them write. Say which to sequence around which.

## Tasks

Two changes that must land together are one task. Two that could land a week
apart are two. Each task's body holds:

- The problem, with evidence.
- Done when: the stop rule.
- What is deliberately not in this task.
- The paths it owns, so a run can tell which tasks may work at once. A path
  nobody can name yet means the scope is open; say so.
- The idea or spec it came from.

Attach each task to its run unit through the tracker's relation, not a line in
a body: a run finds its tasks through the relation. Where one task cannot
start until another lands, record a dependency as well, since the relation
says nothing about order. A task waiting on a dependency is not blocked; leave
its status alone.

## Then

Report what you wrote: the collection, each run unit and each task, with
their IDs, and the dependencies between them. If a write fails partway, stop
and report exactly what was written, so the next run resumes from there.

## Do not

- Write a task whose stop rule you cannot state.
- Give a run unit a status, or nest deeper than run unit and task.
- Open a collection for one run unit.
- Claim or start any of it. Filing and working are separate steps.
