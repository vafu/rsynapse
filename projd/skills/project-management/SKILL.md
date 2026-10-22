---
name: project-management
description: Use when an agent is working on a registered project or needs to read, create, or explicitly update project goals through the proj CLI.
---

# Project-management workflow

Use `proj` as a client of the reusable `projd` service. Do not write its SQLite
database or locus metadata directly. Desktop/window bindings belong to steward.

1. Resolve the registered project with `proj root` or `proj metadata --json`.
   Each Git checkout/worktree is a separate project; non-Git projects have no
   checkout. Read/refresh commands preserve the stored name/CWD/icon and do not
   register directories. Use `proj add PATH --name NAME` for explicit registration.
2. Read the project's checklist with `proj goal list --json` before creating duplicates.
3. Associate your implementation plan with an existing outcome when appropriate.
4. For multi-step work, mark the parent in progress and use `proj goal start SUBGOAL_ID`
   to track the concrete subgoal currently being worked on. Read parent context too.
5. Refresh project metadata after git actions with `proj update`.
6. Verify the success criterion and record supporting evidence in your response
   before explicitly marking an outcome completed. An idle agent is not proof.
7. Leave personal habit judgments to the user; do not mark them completed from
   coding/tool activity. Use deferred when the user explicitly changes the plan.

```sh
proj metadata --json
proj goal list --json
proj goal add --title 'Implement the feature' --description 'Requirements and constraints'
proj goal status GOAL_ID in-progress
proj goal add --parent GOAL_ID --title 'Verify the implementation'
proj goal start SUBGOAL_ID
proj goal update GOAL_ID --description 'Refined requirements'
proj goal note GOAL_ID
# Read/append the returned Markdown path for research, progress notes and evidence.
proj goal complete SUBGOAL_ID
proj goal add outcome1 --kind outcome --title 'Observable outcome' --project "$PWD" \
  --success 'Concrete completion criterion' --priority high
proj goal status outcome1 in-progress
proj update
proj goal status outcome1 completed
```

Use `--date YYYY-MM-DD` when a goal belongs to a particular day. Projd has no knowledge of the shell,
niri, AgentDBus, Graphite or Grafana. This skill uses its public management API;
session association and measurement are separate integrations.

New tasks persist without a required date. Parent IDs are returned by creation;
subtasks inherit the project. Each task has a service-created Markdown note,
which metadata updates preserve. Keep checklist/status changes in `proj`, and
free-form context/logs in the note. Completion of a subtask does not implicitly
complete its parent. `proj goal set NAMED_ID --title TITLE` is idempotent;
`goal show ID` and `goal list --json` are the agent-readable record surfaces.
An in-progress multi-step goal should normally have an in-progress subgoal
associated with the current work. Work directly on a parent only for atomic tasks;
keep its overall scope open until all completion criteria are verified.

Read recorded history/effort through `proj goal history ID`, `goal timeline ID`,
`goal stats ID`, or `proj report --from today --json`. Parent stats include subgoal
work unless `--direct` is set. Keep focused active/idle effort separate from
status elapsed time and completion lead time. Legacy timestamps can be unknown;
`goal instances ID --json` provides immutable IDs for deleted/recreated keys.

Manage generic project-owned labels through the same public CLI:

```sh
proj project labels get --json
proj project labels add bug 'team name' --project /absolute/project --json
proj project labels remove bug --project-id PROJECT_ID --json
```

Without a target flag, labels resolve the current directory's existing project;
they never register it. `--project PATH` and `--project-id ID` are mutually
exclusive. Add/remove take multiple operands atomically and return the resulting
set. Labels trim outer whitespace, reject empty/control strings, and preserve
case and internal spaces; matching is exact. `proj metadata --json` also includes
`labels`. Do not implement client-side read/replace or put labels in locus,
workspace state, goal descriptions, or integration-specific stores.
