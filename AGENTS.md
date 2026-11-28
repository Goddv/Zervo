# Working on Heion

- Keep the repository root checked out on `main`.
- For work on a branch, create its Git worktree inside the repository's
  ignored `.branches/` directory. Run worktree commands from the repository
  root and do branch work in that worktree.

```sh
git worktree add -b codex/short-description .branches/short-description main
```
