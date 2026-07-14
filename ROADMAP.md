# MIRE roadmap

MIRE's canonical plans are versioned under `docs/internal`. Each `plan.md`
defines product intent, architecture, invariants, verification, and release
boundaries. Its sibling `task.md` owns implementation order and dependency edges.

| Release | Outcome                                                                                                                     | Status                   | Plan                                | Tasks                                |
| ------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------ | ----------------------------------- | ------------------------------------ |
| V1      | Local, single-repository review workbench with immutable evidence, static terminal output, and an interactive localhost app | Ready for implementation | [V1 plan](docs/internal/v1/plan.md) | [V1 tasks](docs/internal/v1/task.md) |
| V2      | Multi-repository workspaces, opt-in version-controlled knowledge, sandboxed execution, patches, and semantic providers      | Planned after V1         | [V2 plan](docs/internal/v2/plan.md) | [V2 tasks](docs/internal/v2/task.md) |
| V3      | Local-first forge import, publication, and synchronization                                                                  | Planned after V2         | [V3 plan](docs/internal/v3/plan.md) | [V3 tasks](docs/internal/v3/task.md) |

## Product direction

MIRE is a local, model-independent code-review workbench and evidence ledger. It
turns a Git change into revision-bound findings, evidence, verification history,
human decisions, and portable review handoffs. Machine review remains advisory;
only a human decides how to act on a finding.

The releases are cumulative. Later plans may extend V1's repository-keyed domain
and versioned boundaries, but must not weaken its snapshot, evidence, provenance,
privacy, or human-authority guarantees.

## Documentation contract

- Change a release's product or architecture contract in its `plan.md`.
- Change implementation scope, blockers, or verification in its `task.md`.
- Keep only navigation and cross-release direction in this file.
- Record implementation discoveries in the active release plan before allowing
  them to silently alter later tickets.
