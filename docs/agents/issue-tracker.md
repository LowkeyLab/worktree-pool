# Issue tracker: GitHub

Issues and specs live in GitHub Issues for `LowkeyLab/worktree-pool`.
Use the `gh` CLI from this repository's checkout.

## Operations

- Create: `gh issue create --title "..." --body-file /absolute/body.md`.
- Read: `gh issue view <number> --json number,title,body,labels,comments`.
- List: `gh issue list --state open --json number,title,body,labels,assignees`.
  Use appropriate label filters and paginate when exhaustive discovery matters.
- Comment: `gh issue comment <number> --body-file /absolute/comment.md`.
- Label: `gh issue edit <number> --add-label "<label>"` or
  `--remove-label "<label>"`.
- Close: `gh issue close <number> --comment "..."`.

When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", read the GitHub issue.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub issues and PRs share a number space. Resolve ambiguous references
with `gh pr view <number>`, falling back to `gh issue view <number>`.

## Wayfinding operations

- Map: an issue labelled `wayfinder:map`, containing Notes,
  Decisions-so-far, and Fog.
- Child: a GitHub sub-issue of the map, labelled `wayfinder:<type>`
  where type is `research`, `prototype`, `grilling`, or `task`.
  If sub-issues are unavailable, use a task list in the map and
  `Part of #<map>` in each child.
- Blocking: use native GitHub issue dependencies. Add an edge with
  `gh api --method POST repos/LowkeyLab/worktree-pool/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`.
  Obtain the database ID with
  `gh api repos/LowkeyLab/worktree-pool/issues/<blocker> --jq .id`.
  If dependencies are unavailable, use `Blocked by: #<number>` lines.
- Frontier: inspect all open map children in map order. Select the first
  unassigned child with no open blockers.
- Claim: `gh issue edit <number> --add-assignee @me`.
- Resolve: comment with the result, close the child, and add a context
  pointer to the map's Decisions-so-far.
