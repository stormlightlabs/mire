---
name: fab-handoff
description: Record the state of unfinished work on its task, in the repository's tracker or on its GitHub issue or pull request, so the next session can pick it up, or pick up from the latest handoff. Use for /fab-handoff, when a session ends with work left, or when a session starts by continuing earlier work.
argument-hint: "[task|pick-up <task>]"
---

# Handoff

A handoff is a note on the task that a later session reads to carry on where
this one stopped. It lives with the task, so any machine and any harness can
read it: in the tracker for a tracked task, as the **Tasks** section of
`AGENTS.md` says, or otherwise as a comment on the GitHub issue, or on the pull
request when there is no issue.

`/fab-handoff <id>` writes one. `/fab-handoff pick-up <id>` picks one up.

Write one before clearing context or ending with work left: steps not taken,
or a revise or decision still owed. A pull request waiting on the owner's
review, or a check still pending, is not work left: the session reports that
state and ends without a handoff.

Work with no task and no pull request gets one first: in the tracker where the
**Tasks** section names one, a GitHub issue otherwise, or ask the operator
where it belongs.

## Writing

For a tracked task, store the note as the **Tasks** section says. When the
tracker keeps tasks in files in the repository, run it in the task's worktree,
never the primary checkout, and commit and push the changed files to the
task's branch.

For an issue or a pull request, post the note as a comment opening with
`Handoff:`, through the Bot comment workflow on a pull request and with
`gh issue comment` or `add_issue_comment` on an issue. Write the note in the
session's scratch directory, never inside the repository.

```markdown
Written <YYYY-MM-DD> from <branch or worktree>.

## State

Each branch, worktree and pull request, with its current head and check state.
Completed review passes, the SHA each read, findings, verifier verdicts and
revisions still needed. Link their results rather than copying them.
If the task was started over the tracker's refusal, say so and name what it
was forced past.

## Done

What this session finished, by commit or pull request.

## Next

The next unfinished step and what it depends on. Worker identities, run
paths and how to check or stop any worker left running; normally wait for
workers or stop them before handing off. Do not launch a replacement until
the earlier worker is known to have stopped.

## Waiting on the operator

Checks, merges and decisions only the operator can make.
```

Confirm the note was stored, then give the operator its link or task ID and
the pickup command. On Pi, use `/new` or restart Pi, then send
`/skill:handoff pick-up <task-or-pr>`. `/resume` restores a Pi conversation;
it is not needed to pick up a task from another harness.

Leave out a section with nothing in it. Record what a session cannot read back
cheaply; the task, the diff and the git log are already there to read. Use the
`fab-writing-docs` skill for the prose.

## Picking up

For a tracked task, read the task and its latest handoff and resume it as the
**Tasks** section says, in the task's worktree, committing any changed task
file with the work. When the tracker says the task cannot start for a reason
other than the handoff itself, such as a blocker added since, stop and tell
the operator the reasons. Do not complete, unblock or resume other tasks to
clear them; the handoff note is input, not leave to change other tasks.
Override the tracker only when the operator asks after seeing the reasons. An
earlier override does not carry over: the reasons may be new.

For an issue or a pull request, read its newest comment opening with
`Handoff:`. A handoff describes the repository
when it was written, so check each claim the next step depends on: that a branch
and worktree exist, the state of each pull request, and that `main` holds what
the note says merged. Where the repository disagrees, the repository is right.
Say what changed before going on. Check any recorded worker is stopped or
attach to its existing run before dispatching another one. Resume the next
unfinished step, keeping completed passes only when the current head and
`fab-storm`'s base-only change rule support them. If review evidence is missing
or conflicting, ask rather than marking a pass complete or rerunning it.
A merged PR needs local refresh and cleanup, not another review sequence;
follow `fab-implement`'s **After merge** steps. A new task starts separately.

## Do not

- Commit a handoff or put it in a pull request's body. A decision that has to
  outlive the work belongs in the task, its issue or its spec.
- Keep the next steps in the harness's own memory. Another harness, or another
  machine, cannot read it.
