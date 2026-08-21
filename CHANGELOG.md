# Changelog

## [1.1.6]

### Added

- Smart series and season auto-grouping with tab navigation (All Episodes, Season 1, Season 2, Specials) and natural numeric sorting.
- "Next Up" progression card in Continue Watching for instant 1-click continuation of series episodes.
- "Binge Series / Play All" button on season headers to launch sequential continuous MPV playlist playback.
- Automatic media format and quality badge tagging (4K UHD, 1080p, 720p, DV, HDR10+, 10-bit, HEVC, AV1, Dual Audio, Atmos).
- High-resolution transparent vector-grade application and installer icon across all desktop and installer sizes.

### Optimized

- Modernized slim, native Windows system tray menu with high-utility actions (Resume Video, Downloads, Upload, Check Updates, Settings, Open, Exit).
- SQLite concurrency with WAL mode, normal synchronous setting, and 5000ms busy timeout to eliminate lock contention during parallel chunk uploads.
- O(1) BTreeMap streaming ring buffer eviction for zero-latency high-bitrate media playback.
- Zero-copy byte slicing on HTTP range requests to minimize memory allocations during video seeking.
- Unified atomic mutex state for batch caption generation queue.

## [1.1.5]

### Fixed

- Fixed MPV player header and chapter bar media title display by introducing named streaming routes, scoped per-file CLI options (`--{ ... --}`), and inline `Content-Disposition` headers.
- Fixed Continue Watching / Recent Watch synchronization when navigating to previous/next episodes directly inside the MPV player via backend stream playback events.
- Added automatic resolution for bundled local MPV binary before PATH fallback.

## [1.1.4]

### Fixed

- Fixed main content area overflowing to the right when a folder is opened, causing empty-folder upload UI and TopBar menu buttons to be partially hidden.
- Fixed TopBar `overflow-hidden` clipping all icon buttons (Settings, Analytics, Logs, etc.) — only the search bar was visible.
- Fixed MPV player displaying incorrect title and streaming from the wrong folder when a file lives in a different folder than the one currently active in the sidebar.
- Fixed Recent Watch History "Resume" opening the first file of the currently-viewed folder instead of the actual resumed file when that file belongs to a different folder.

## [1.1.3]

### Fixed

- Resolved login, auth session restoration, and reconnection stability.
- Fixed MTProto session restore locks and improved authentication error recovery.

## [1.1.2]

### Fixed

- Instant session restoration (<5ms) using local SQLite pre-authorization.
- Restored v1.1.1 native auth pipeline, eliminating MTProto runner deadlocks and connection hangs.
- Enforced E.164 international phone number format validation (`+` country code prefix).

## [1.2.2]

### Fixed

- Instant session restoration (<5ms) using local SQLite pre-authorization.
- Eliminated MTProto runner deadlocks and connection hangs on login and session restore.
- Enforced E.164 international phone number format validation (`+` country code prefix).
- Standardized MTProto connection parameters for reliable Data Center handshake.

## [1.2.1]

### Fixed

- Prevented app startup freeze on "Restoring session..." splash screen by adding pre-authorization checks and timeouts.
- Adjusted TopBar flex layout so search box dynamically scales without covering control menu buttons.

## [1.2.0]

### Changed

- Migrated Telegram API client to `grammers` crates.io v0.10.0.
- Upgraded system metrics library `sysinfo` to v0.39.
- Updated all frontend npm dependencies and pinned CI/CD GitHub Actions toolchain SHA.
- Refactored Telegram peer resolution and media handling for grammers 0.10 compatibility.

## [1.1.1]

### Added

- Enhanced Windows 11 System Tray menu with 4 quick controls (`🍿 Continue Watching`, `⏯️ Pause/Resume Transfers`, `Open`, `Exit`).

### Changed

- Updated GitHub Actions release workflow runner to Node.js 26 (`FORCE_JAVASCRIPT_ACTIONS_NODE_TYPE: node26`).
- Translated all hardcoded UI components and Rust backend error messages to English.

## [1.1.0]

### Fixed

- Legacy split part file leakage in folder views ([teledrive-part], [telegram-drive-part], etc.).
- Inject media filename into MPV sidecar titles instead of numeric message IDs.
- Natural ascending sort order for playlists (Ep 01 -> Ep 02 -> Ep 10) and directional arrow key alignment.
- Safe Option map handling for API sparse fieldsets.
- Official v1.1.0 version badge display in the top-left sidebar header.

## [1.0.0]

### Added

- Windows notification-area controls, storage analytics, recent-watch history,
  MPV resume support, and detailed transfer diagnostics.
- Split upload for files over `2_000_000_000` bytes with manifest validation
  across streaming, download, move, and delete operations.

### Changed

- Established the TeleStash 1.0 Windows-only application identity.
- Removed unused parallel transfer modules and proxy, VPN, SOCKS, optimizer,
  and application bandwidth-throttle paths.
- Made the low-overhead rendering policy the application default while keeping
  progress indicators and interaction feedback visible.
- Improved dashboard empty, search, queue, dialog, and keyboard interaction
  states.

### Fixed

- The release workflow now validates version alignment and changelog metadata,
  builds signed assets before creating a release, rejects missing updater
  artifacts, replaces duplicate assets on reruns, and awaits publication.
