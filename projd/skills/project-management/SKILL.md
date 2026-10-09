---
name: project-management
description: Use when an agent is working on a registered project or needs to read, create, or explicitly update project goals through the proj CLI.
---

# Project-management workflow

Use `proj` as a client of the reusable `projd` service. Do not write its SQLite
database or locus metadata directly. Desktop/window bindings belong to steward.

1. Resolve the working context with `proj root` or `proj metadata --json`.
2. Read the day's goals with `proj goal list --json` before creating duplicates.
3. Associate your implementation plan with an existing outcome when appropriate.
4. Explicitly mark it `in-progress` when work begins.
5. Refresh project metadata after git actions with `proj update`.
6. Verify the success criterion and record supporting evidence in your response
   before explicitly marking an outcome completed. An idle agent is not proof.
7. Leave personal habit judgments to the user; do not mark them completed from
   coding/tool activity. Use deferred when the user explicitly changes the plan.

```sh
proj metadata --json
proj goal list --json
proj goal add outcome1 --title 'Observable outcome' --project "$PWD" \
  --success 'Concrete completion criterion' --priority high
proj goal status outcome1 in-progress
proj update
proj goal status outcome1 completed
```

Use `--date YYYY-MM-DD` for another day. Projd has no knowledge of the shell,
niri, AgentDBus, Graphite or Grafana. This skill uses its public management API;
session association and measurement are separate integrations.
