# Folder-aware media library

Folder options let a user choose Series, Movies, or General. These presentation
preferences live in the existing settings store and use stable folder IDs;
renaming a folder does not reset its presentation. Explicit per-folder layouts
override the Series list / Movies poster defaults. General uses the global view.
Version-1 settings backups without presentation preferences remain supported.

Series expose all files, detected seasons, and other files, including unrecognized
video names and companion documents. Movies expose Play and Details and use real
available image previews or a title/type fallback. There is no external artwork
or catalog dependency. Playback uses native MPV.

Selections and file actions identify files by folder and message together. Bulk
operations retain each source folder; dragging from cross-folder search/favorites
is intentionally replaced by the explicit Move action to avoid ambiguous sources.

Watch history now uses a composite folder/message key. Its SQLite upgrade runs
atomically, preserves existing row values, and rolls back on failure. The old
schema's already-overwritten history cannot be reconstructed by this upgrade.
No migration is needed for folder presentation preferences themselves.

## Validation

- Frontend TypeScript/Vite production build passed.
- Combined Vitest run: 64 tests passed; subsequent MPV same-ID/different-folder
  regression and launch-state checks: 2 passed.
- `npm run test:watch-history-sql`: 4 production-SQL tests passed, covering data
  preservation, idempotence, rollback, composite operations and history limits.
- Browser inspection used sample movie files and auth in dark/light mode.
- The local browser preview fell back to a system font because the shared
  dependency junction was outside Vite's serving allowlist. The production build
  includes the bundled Outfit font; its native rendering still needs checking.
- No local Rust compilation, live Telegram mutations, installer smoke test,
  real MPV playback test, or live-profile settings restore was performed.

The final independent review identified the MPV identity reset issue; it was fixed
and regression-tested. Jev's final gate escalated (safe-to-apply 0.22), with low
confidence on scope/test coverage and no concrete additional defect specified.
This is not an automatic approval or a release certification. Rust/Windows CI
and native acceptance remain necessary before distributing an installer.
