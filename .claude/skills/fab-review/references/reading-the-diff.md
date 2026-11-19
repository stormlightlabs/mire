# Reading the diff

Why a review pins a commit, fetches before reading, and keeps its own files
outside the repository.

## Pin the commit

A branch moves while a review reads it. Take the head commit once and review
that, so every finding describes one state of the tree:

```sh
git fetch origin <branch> main
commit=$(git rev-parse origin/<branch>)
git diff origin/main..."$commit"
git show "$commit":<path>
git log --oneline origin/main.."$commit"
```

The fetch is not optional. A cloud container clones a few branches at a
shallow depth, so the branch under review is usually absent and `git show`
fails on a commit the clone has never had.

## Do not read the working tree

`Read`, `Grep` and `Glob` read the working tree, which holds whatever branch
the session sits on and is rarely the one under review. Where reading a whole
tree is worth it, take a worktree that cannot move:

```sh
git worktree add --detach ../<repo>-worktrees/review-<n> "$commit"
```

Remove it when the pass ends.

## Write nothing inside the repository

A reviewer shares a checkout with whoever is writing there, and a findings file
left in it is untracked work belonging to nobody. Keep drafts in the session's
scratch directory.

## Transport

A local run uses `gh`. A cloud session, which has no `gh`, uses the GitHub MCP
tools. Use one transport for the whole pass.

```sh
gh pr view <n> --json title,body,headRefName,baseRefName,files
gh pr diff <n>
```

Through MCP: `pull_request_read` methods `get`, `get_files` and `get_diff`.
