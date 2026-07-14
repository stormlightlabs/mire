# MIRE task index

Implementation work is maintained beside each versioned plan:

- [V1 implementation tasks](docs/internal/v1/task.md)
- [V2 implementation tasks](docs/internal/v2/task.md)
- [V3 implementation tasks](docs/internal/v3/task.md)

## Current frontier

Start with **V1-01 — Establish private review state and session lifecycle** in
the [V1 task list](docs/internal/v1/task.md).
It is the only ticket with no blockers. Once complete, V1-02 and V1-03 form the
next parallel frontier.

V2 starts after V1-26, with V2-00 and V2-05 as its parallel frontier. V3 starts
after the V2-14 release gate, with V3-01. Within a release, work any ticket whose
declared blockers are complete; milestone order does not create an unstated
dependency.

Prefer one ticket per fresh implementation context. Seed that context with the
version's `plan.md`, the selected ticket, and only the current files needed for
the work.
