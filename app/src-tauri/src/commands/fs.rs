//! Files, folders, transfers, and the split-upload engine.
//!
//! This file is only the module root. Each responsibility lives in a submodule:
//!
//! - [`folders`] — folder (Telegram channel) lifecycle and discovery.
//! - [`listing`] — folder file cache, delta sync, and both search paths.
//! - [`files`]  — single-file rename, delete, and move.
//! - [`download`] — single-file and split download into a local path.
//! - [`upload`] — upload pipeline, progress reporting, and pause/cancel control.
//! - [`split`]  — split-upload primitives: manifests, parts, resume snapshots.
//!
//! The glob re-exports keep every previous `crate::commands::fs::…` path
//! resolving, so no caller changed when this was split up. They are globs on
//! purpose: `#[tauri::command]` also emits hidden `__cmd__*` items that
//! `generate_handler!` looks up here, and an explicit re-export list silently
//! drops them.

mod download;
mod files;
mod folders;
mod listing;
mod split;
mod upload;

pub use download::*;
pub use files::*;
pub use folders::*;
pub use listing::*;
// `split` exposes only `pub(crate)` helpers — it has no Tauri commands of its
// own — so a `pub use` glob here would warn that it re-exports nothing public.
pub(crate) use split::*;
pub use upload::*;
