---
name: review
description: Review a pull request or branch for defects and report the findings. Runs a standard pass or an adversarial pass. Use for /review, /adv-rev, code review, PR review, or when asked to check a diff before merge.
argument-hint: "[pr-number|branch] [standard|adversarial]"
---

# Review

Review a diff and report findings. Do not edit code during a review pass; the
`revise` skill owns changes.

The `storm` skill runs one standard pass and then one adversarial pass, each in
a fresh agent and each followed by a verifier and a revise. A pass reports to
whoever invoked it, and posts only when they ask.

## Pick the target and the pass

The argument is a pull request number, a branch name, or empty for the current
branch, then the pass. Without one, the pass is standard.

| Pass        | Brief                                                                                                       |
| ----------- | ----------------------------------------------------------------------------------------------------------- |
| Standard    | Correctness, edge cases, security, concurrency, performance, API compatibility, test coverage, readability. |
| Adversarial | Worst case only: security holes, races, unhandled input, violated invariants, tests that cannot fail.       |

The adversarial pass runs after the standard pass and is handed its findings.
It reports again only those that the code still gets wrong, and otherwise
looks for what the standard pass would miss. The `adversarial-reviewer` agent
lists what it hunts and what it owes each finding.

### Complexity

Both passes weigh complexity, because a defect is cheaper to prevent than to
find. Ask of each change:

- Is this harder than the problem it solves? Name the simpler version and what
  it would cost.
- Does it add an interface, a layer, or a configuration point that one
  concrete helper would cover?
- How many things must a reader hold at once to know this is correct?
- Does it repeat something the codebase already does, under a new name?
- Could a check replace the care this asks of the next person to touch it?

Report complexity the way you report a defect: what it costs, and what to do
instead. "Simpler would be better" without a concrete alternative is not a
finding.

## Gather context first

Read the changed files around the diff, not only the diff. Read the task the
change is for: the branch name, the pull request body or the first commit names
it. For an arcl task, `arcl context <id>` gives its acceptance criteria and
spec; for an issue, `gh issue view <n>` or `issue_read`; otherwise the pull
request body is the task.

`references/reading-the-diff.md` gives the commands. In short: fetch the
branch, pin its head commit once and review that, use one transport (`gh`, or
the GitHub MCP tools in a cloud session), and read the change through git
rather than the working tree, which may hold another branch.

## Judge against something

A finding needs a source. Rank them:

1. The task's acceptance criteria and its spec.
2. `AGENTS.md`, and the conventions the surrounding code already keeps.
3. Tests that exist, tests that should exist, and tests that cannot fail.
4. Behavior a user can observe.

Taste alone is a `nit` at most. A disagreement between models is not a
finding.

### Prose

Documentation in the diff is a change like any other, and so are the pull
request title and body, the branch's commit messages and the comments posted.
Judge them against the `writing-docs` skill and its tells catalogue.

| What you find                                     | Severity |
| ------------------------------------------------- | -------- |
| Documentation contradicts the code it describes   | `high`   |
| User-visible behavior changed, documents did not  | `medium` |
| A catalogued tell, or one pattern used throughout | `medium` |
| Past a soft limit or a length target              | `low`    |
| Heading case, term drift                          | `nit`    |

A pattern repeated through a document is one finding naming the pattern, not
one per instance.

## Finding format

One line per finding:

```text
<severity> · <path>:<line> — <problem> → <why it matters> → <fix direction>
```

| Severity  | Means                                                                |
| --------- | -------------------------------------------------------------------- |
| `blocker` | Merging causes data loss, a crash, a security hole, or a regression. |
| `high`    | Wrong behavior in a case the change is supposed to handle.           |
| `medium`  | Wrong behavior in an unhandled case, or a missing test for one.      |
| `low`     | Works, but will cause a defect later.                                |
| `nit`     | Style or naming inside the repository's conventions.                 |

State what breaks and with what input. A finding that cannot name a failing
case is speculation; drop it or mark it `nit`.

## Keep the report readable

- Most severe first, with no preamble and no closing summary.
- Under 200 words. Past ten findings, give the ten that matter and say how many
  you left out. Cut a finding rather than compressing ten into denser prose.
- Cite `<path>:<line>` instead of quoting the diff back.
- Group repeats: one finding naming every site.
- Open with one line naming the pass, the target and what you ran as:
  `Adversarial pass on #12 at 4f2a91c · <model> · <effort>`. Use the model
  identifier the harness reports.

Say `No findings` when there are none rather than inventing one.

## Stop conditions

- Do not repeat a finding an earlier pass raised and a `revise` pass fixed,
  unless the fix did not hold.
- If the same finding survives two `revise` passes without its cause changing,
  stop and escalate to the user.

## Do not

- Edit files, commit, push, approve, or merge.
- Review a diff you wrote in this same session without saying so.
