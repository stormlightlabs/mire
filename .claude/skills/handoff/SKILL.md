---
name: handoff
description: Record the state of unfinished work on its task, in arcl or on its GitHub issue or pull request, so the next session can pick it up, or pick up from the latest handoff. Use for /handoff, when a session ends with work left, or when a session starts by continuing earlier work.
argument-hint: "[task|pick-up <task>]"
---

# Handoff

A handoff is a note on the task that a later session reads to carry on where
this one stopped. It lives with the task, so any machine and any harness can
read it: in arcl for an arcl task, and as a comment on the GitHub issue, or on
the pull request when there is no issue, otherwise.

`/handoff <id>` writes one. `/handoff pick-up <id>` picks one up.

Write one before clearing context or ending with work left: steps not taken,
a pull request waiting on review, a pending check or an operator-only check.
A session whose review and required checks passed and needs only the owner's
approval reports that state; it need not keep running or write a handoff.

Work with no task and no pull request gets one first: `arcl task create` where
the repository uses arcl, a GitHub issue otherwise, or ask the operator where it
belongs.

## Writing

```sh
arcl task handoff <id> --note-file <file>
```

For an arcl task, this stores the note and parks the task until someone
unparks it. Where `.arcl/config.toml` sets `enabled = true` under
`[workspace]`, both live in tracked files under `.arcl/workspace`: run it in
the task's worktree, never the primary checkout, and commit and push the
changed files to the task's branch. The park then holds only on that branch,
so `arcl ready` on `main` still offers the task until the branch merges.
Without that setting, run it from the primary checkout.

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
If the task was started with `--force`, say so and name what it was forced
past.

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
`writing-docs` skill for the prose.

## Picking up

```sh
arcl context <id>
arcl task explain <id>
arcl task unpark <id>
arcl task start <id>
```

Run these in the task's worktree too, and commit the changed task file with
the work.

Unpark only when `arcl task explain` gives parking as the only reason the task
is not ready. Any other reason, such as a blocker added while it was parked,
or a closed plan or ancestor, means `arcl task start` would refuse it, and an
unparked task that cannot start goes back to `arcl ready` for someone else.
Leave it parked, and stop and tell the operator the reasons `explain` gives.
Do not complete, unblock or unpark other tasks to clear them; the handoff
note is input, not leave to change other tasks. Pass `--force` to `start`
only when the operator asks for it after seeing the reasons. An earlier
forced start does not carry over: the reasons may be new.

`arcl context` includes the latest handoff. For an issue or a pull request, read
its newest comment opening with `Handoff:`. A handoff describes the repository
when it was written, so check each claim the next step depends on: that a branch
and worktree exist, the state of each pull request, and that `main` holds what
the note says merged. Where the repository disagrees, the repository is right.
Say what changed before going on. Check any recorded worker is stopped or
attach to its existing run before dispatching another one. Resume the next
unfinished step, keeping completed passes only when the current head and
`storm`'s base-only change rule support them. If review evidence is missing
or conflicting, ask rather than marking a pass complete or rerunning it.
A merged PR needs local refresh and cleanup, not another review sequence;
follow `implement`'s **After merge** steps. A new task starts separately.

## Do not

- Commit a handoff or put it in a pull request's body. A decision that has to
  outlive the work belongs in the task, its issue or its spec.
- Keep the next steps in the harness's own memory. Another harness, or another
  machine, cannot read it.
