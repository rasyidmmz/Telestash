# ADR 0003 — Tier 3 file identity key

- Status: Accepted
- Date: 2026-09-11
- Scope: Favorites, manual tags, sort persist, duplicate fast path (Tier 3 / v1.7.0)

## Context

Favorites/tags must survive restarts and correctly target the same library entry the
user sees in the grid. Telegram message ids are unique **per chat**, not globally.
Split uploads (`> 2_000_000_000` bytes) store many part messages plus one
`.tdmanifest.json`; the UI presents them as a single file (the manifest).

`watch_history` already keys on `file_id` alone — acceptable risk for resume, wrong
for long-lived user labels.

## Decision

Use **`(folder_id, message_id)`** as the identity key for Tier 3 user data.

- Saved Messages / home → `folder_id = NULL` (same sentinel as the stream route
  `/home` / `/me`).
- Split media → `message_id` is the **manifest** message, never a part.
- On file delete (including split cleanup), purge matching favorite/tag rows, same
  pattern as `purge_file_side_tables`.
- If rename/forward assigns a new `message_id`, remap rows in that path.

## Consequences

- Tables and IPC commands take `folder_id: Option<i64>` + `message_id: i32`.
- Queries are slightly more verbose than `file_id` alone.
- Must not reuse `quality_tag` on `watch_history` for user tags (different meaning).

## Alternatives considered

- `file_id` only — collides across folders; ignores split manifest.
- Content hash as key — expensive; changes on re-upload of same logical file.
