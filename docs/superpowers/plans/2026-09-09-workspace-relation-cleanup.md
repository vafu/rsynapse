# Workspace Relation Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all relations owned by a deleted Niri workspace without deleting its project metadata.

**Architecture:** Locus exposes a generic subject-wide clear operation. Niri D-Bus invokes it for each removed numeric workspace before deleting that workspace's live D-Bus object.

**Tech Stack:** Rust, zbus, Tokio, Locus relation store, Niri IPC projection.

**Spec:** `docs/superpowers/specs/2026-09-09-workspace-relation-cleanup-design.md`

## Global Constraints

- Do not add project-specific relation names to `niri-dbus`.
- Preserve project metadata targets and clear only records whose subject is the deleted workspace endpoint.
- Use typed `RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, id)` values.
- Propagate Locus cleanup failures through the existing Niri reconnect path.

---

### Task 1: Add generic Locus subject cleanup

**Files:**
- Modify: `locus/src/store.rs`
- Modify: `locus/src/service.rs`
- Modify: `locus/src/lib.rs`
- Test: `locus/src/store.rs`

**Interfaces:**
- Produces: `RelationStore::clear_subject(&RelationEndpoint) -> io::Result<Vec<RelationRecord>>`
- Produces: `Relations::clear_subject(subject: RelationEndpoint) -> zbus::Result<u32>`

- [ ] **Step 1: Write the failing store test**

```rust
assert_eq!(store.clear_subject(&workspace(7))?.len(), 2);
assert!(store.list("org.rsynapse.workspace.project").is_empty());
assert_eq!(store.list("org.rsynapse.project.metadata").len(), 1);
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path locus/Cargo.toml clear_subject`

- [ ] **Step 3: Implement the minimal store and D-Bus method**

```rust
async fn clear_subject(&self, subject: RelationEndpoint, ...) -> fdo::Result<u32>
```

Remove matching records atomically, emit `RelationRemoved` for each, and return the count.

- [ ] **Step 4: Run Locus tests**

Run: `cargo test --manifest-path locus/Cargo.toml`

### Task 2: Clear deleted workspace subjects from Niri D-Bus

**Files:**
- Modify: `niri-dbus/Cargo.toml`
- Modify: `niri-dbus/src/service.rs`
- Test: `niri-dbus/src/service.rs`

**Interfaces:**
- Consumes: `locus::RelationsProxy::clear_subject(RelationEndpoint)`
- Consumes: `ObjectDelta.removed.workspaces`

- [ ] **Step 1: Write the failing endpoint test**

```rust
assert_eq!(workspace_subject(19), RelationEndpoint::stable_key(keys::NIRI_WORKSPACE_ID, "19"));
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path niri-dbus/Cargo.toml workspace_subject`

- [ ] **Step 3: Implement the minimal cleanup before object removal**

```rust
for workspace in sorted(delta.removed.workspaces) {
    clear_locus_subject(&self.connection, workspace).await?;
    // remove WorkspaceInterface
}
```

- [ ] **Step 4: Run full verification**

Run: `cargo fmt --check --manifest-path niri-dbus/Cargo.toml && cargo test --manifest-path locus/Cargo.toml && cargo test --manifest-path niri-dbus/Cargo.toml`
