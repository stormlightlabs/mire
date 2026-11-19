---
name: fab-rubber-duck
description: Work out a design with the person in rounds of questions, then record the decisions as an idea, or as a spec when one decision must come before any work can be cut. Use for /fab-rubber-duck, design discussion, or working out an approach before any task exists.
argument-hint: "[topic]"
---

# Rubber duck

A design conversation with the person, ending in a written record of what was
decided. The interview follows Matt Pocock's `grilling` skill
(github.com/mattpocock/skills, `skills/productivity/grilling`).

## The interview

Map the design as a tree of decisions: each decision opens the ones that
depend on it. The frontier is every open decision whose prerequisites are
settled, the questions you can ask without guessing at an answer you have not
heard.

Work in rounds. Ask the whole frontier in one message, numbered, each with
your recommended answer and why. Word each question so that "yes" accepts the
recommendation. Then stop and wait. A question that depends on another one
still open this round waits for the next round.

Each set of answers changes the tree. Recompute the frontier and ask again.

Finding facts is your job. When a question turns on what the code does, what a
tool supports, or what a source says, look it up, or send a sub-agent to, and
ask only what follows from the answer. Cite the source of a claim that carries
weight. The decisions are the person's.

Argue with the idea. Say when a premise is wrong, including one the person
gave, and name the tradeoff you are recommending against rather than listing
options. Do not ask again about what is settled.

The interview ends when the frontier is empty and the person confirms you
share an understanding. Write nothing until they do.

## Idea or spec

Record a spec when work cannot be cut yet: you cannot write a task's stop rule
without first making a decision, and several tasks would each make it their
own way. A spec holds that decision, what it rules out, the shape the tasks
will share (the pieces, who owns what, the terms the tasks use), and what is
out of scope. Otherwise record an idea; most chores and fixes need no spec.

Either one holds:

- The problem, with evidence.
- The decisions, one line each with the reason.
- What is still open, and what would settle it.
- Sources, with what each supports.

Record decisions, not the conversation. Keep reasoning that would be costly to
rebuild. Neither holds a task list or code; the tasks are `fab-decompose`'s.

## Where it goes

Write it through the commands the **Tasks** section of `AGENTS.md` names for
an idea and a spec. Without that section, write `docs/ideas/<name>.md` or
`docs/specs/<name>.md` in the current worktree, where `<name>` is a few words
of the title joined with dashes.

Write the record and stop. A record that is a file, through the **Tasks**
section or under `docs/`, is committed only when the person asks: then on a
new `claude/<topic>` branch, pushed so Branch PR open opens the pull
request, never on `main`. A spec is reviewed in that pull request before work
is cut from it.

Use the `fab-writing-docs` skill for the prose.

## Then

Report what you recorded and where. Once a spec is on `main`, or an idea has
criteria someone could check, it goes to `fab-decompose`.

## Do not

- Write code, or create worktrees.
- Commit, or create a branch, unless the person asks.
- Create tasks. That is `fab-decompose`.
- Ask the person for a fact you could look up.
