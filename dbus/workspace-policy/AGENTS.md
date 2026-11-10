# workspace-policy

Pure workspace naming/project/visibility policy shared by shell and launcher.

- Keep it data-only and side-effect free.
- Do not add D-Bus clients; inputs are relation records and typed snapshots.
- Both consumers must agree on visibility, search, current-workspace, and title
  behavior here rather than duplicating it.
