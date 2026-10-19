# Worktree pool

Worktree pool assigns reusable coding checkouts. These terms distinguish refresh requirements from checkout selection.

## Language

**Refresh baseline**:
The remote branch that acquisition must fetch successfully before it can assign a checkout.
_Avoid_: Task reference, checkout target

**Task reference**:
The optional Git reference that selects the commit for the assigned checkout.
_Avoid_: Refresh baseline

**Resolved commit**:
The fixed Git commit that acquisition uses for the assigned checkout.
_Avoid_: Moving reference, branch name
