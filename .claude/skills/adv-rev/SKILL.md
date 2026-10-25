---
name: adv-rev
description: Adversarial review pass on a pull request or branch. Use for /adv-rev, or when asked for a worst-case or adversarial review.
argument-hint: "[pr-number|branch]"
---

# Adversarial review

Use the `review` skill in adversarial mode, on the target in the argument: a
pull request number, a branch name, or empty for the current branch.

Ask the invoker for the standard pass's findings if you were not handed them,
and check each one before looking further. The `adversarial-reviewer` agent
lists what the pass hunts.
