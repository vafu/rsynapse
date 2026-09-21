# Workspace Relation Cleanup Design

## Goal

Remove all Locus relations whose subject is a Niri workspace stable ID when that workspace is removed.

## Design

Locus gains a generic `ClearSubject` D-Bus method. It atomically deletes every record with the supplied subject, emits `RelationRemoved` for each record, and returns the number removed. It does not delete target objects or project metadata records.

`niri-dbus` uses the existing workspace-removal delta to call `ClearSubject` for each removed `org.rsynapse.niri.workspace.id` endpoint before removing the corresponding live D-Bus object. This keeps Niri responsible for workspace lifetime and Locus responsible for relation storage; neither service contains project-specific policy.

## Error Handling

If Locus cannot be reached or rejects cleanup, `niri-dbus` returns the error through its existing reconnect loop, preserving the removal delta for retry rather than silently retaining stale associations.

## Verification

Unit-test Locus storage/API behavior for subject-wide clearing and test the Niri cleanup helper against the typed workspace endpoint. Run both crate test suites.
