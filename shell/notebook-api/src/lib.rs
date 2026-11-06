//! Vendored Remarked notebook contract used by the shell task/notes surface.
//!
//! Kept deliberately small: only the D-Bus wire types the shell reads and
//! writes. The Remarked service remains authoritative; this crate pins the
//! client-side contract so the shell builds from a clean checkout.

pub mod note;
pub mod notebook_scope;
