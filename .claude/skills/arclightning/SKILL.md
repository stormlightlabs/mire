---
name: arclightning
description: Pick up, work, hand off, and complete tasks tracked by Arc Lightning (`arcl`). Use in a project with an `.arcl/` directory, when asked what to work on next, to start or finish an arcl task, or to leave a resume note for the next session.
---

# Arc Lightning

Arc Lightning tracks a project's tasks in `.arcl/`. Drive it through the `arcl`
CLI with `--json`, and read fields from the envelope rather than from terminal
text. `arcl` decides readiness and which transitions are allowed. Ask it
instead of working that out yourself.

When the `arcl-mcp` server is connected, call its tools instead of the
commands. Each step below names the tool in brackets. A tool returns the same
envelope as its command, as structured content. Its input schema lists the
fields it takes, which do not always match the command's options.

## Read the envelope

Every `--json` command writes one object to standard output:

```json
{ "format_version": <n>, "data": { "...": "..." } }
```

The version depends on the command. Check it before reading `data`. A failure goes to standard error
as `{"format_version": 1, "error": {"code", "exit_code", "message"}}`, and the
command exits with that nonzero `exit_code`. A failed tool call returns the
same object with `isError` set. The
[automation guide](https://github.com/stormlightlabs/arclightning/blob/main/apps/website/src/content/docs/guides/automation.md)
lists the version each command returns, the readiness reason kinds, and their
ID fields.

## Work the loop

1. Find work. First resume your own parked work:
   `arcl --json list --kind task --status parked` [`list`] lists tasks a
   session handed off. Otherwise `arcl --json ready` [`ready`] returns
   `data.tasks` in order, and `arcl --json next` returns the first as
   `data.task`. An empty list means nothing can start. The tools return at
   most `limit` records and a `data.next_cursor` to pass as `cursor` while
   more remain.
2. Start it. `arcl --json task start <id>` [`task_start`] moves the task to
   in progress. If it refuses with `invalid_request`, run
   `arcl --json explain <id>` [`explain`] and read `data.readiness.reasons`.
   Do not pass `--force` unless the person asks. The tool has no force.
3. Read the context. `arcl --json context <id>` [`context`] returns the task
   with its ancestors, spec, plan, phase, blockers, dependents, readiness, and
   earlier evidence. `data.task.handoff` holds the last resume note, and
   `data.task.body` holds the acceptance criteria.
4. Do the work the task describes.
5. Finish or hand off:
   - Done: `arcl --json task complete <id> --evidence "<proof>"`
     [`task_complete`], where the evidence says what shows the work is done.
     `data.unblocked` lists the tasks this made ready. Start the next one from
     there or go back to step 1.
   - Not done: `arcl --json task handoff <id> --note "<note>"`
     [`task_handoff`], where the note says where the work stands and what is
     next. This parks the task. The next session finds it in step 1, runs
     `arcl --json task unpark <id>` [`task_unpark`], then starts it again and
     reads the note from the context.

Use `--evidence-file -` and `--note-file -` to pass longer Markdown on
standard input.

## Record new work

When you find work outside the task, capture it instead of doing it:

```sh
arcl --json capture create "<title>" --body "<what and why>"
```

The tool is `capture_create`. The server also has `capture_promote`, `show`,
`spec_create`, `plan_create`, `task_create`, `task_update` and `note_create`,
for `arcl capture promote`, `arcl show`, `arcl spec create`,
`arcl plan create`, `arcl task create`, `arcl task update` and
`arcl note create`.

Run `arcl <command> --help` for every flag, and `arcl check` to validate the
project.
