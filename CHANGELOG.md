# Changelog

## [1.3.4]

### Fixed

- **Collections drawer scrolling actually works now**: the v1.3.3 height-bound CSS targeted `.vault-folder-drawer > aside`, but the sidebar sits inside the `.vault-drawer-body` wrapper, so the selector never matched and the folder list stayed clipped. The rule now targets `.vault-drawer-body > aside`, restoring the internal folder/channel list scroll while the sync/logout footer stays pinned.

## [1.3.3]

### Fixed

- **Rail popovers clipped at the screen edge**: the sync-status and flood-wait popovers opened centred above a button that sits ~27px from the left screen corner, so they were half-hidden in fullscreen. They now open to the right of the button, fully visible. Rail tooltips follow the same activity-bar pattern (open to the right, vertically centred).
- **Command bar icons squashed by the selection strip**: selecting files made the nowrap selection strip push and compress the fixed-size icon buttons (flex-shrink was left at its default), clipping the rightmost icons. Icon buttons, the selection strip, and the divider can no longer shrink; the context label and search bar absorb the space instead, and everything restores cleanly on deselect.
- **Inconsistent secondary type scale**: section labels and eyebrows (Channel, Continue Watching, sort headers) now share one 11px label size with matching uppercase/tracking; micro-badges sit at 10px instead of 9–9.5px.

## [1.3.2]

### Fixed

- **Search hides split parts in all cases**: parts of large files could appear in global search results when Telegram returned messages without captions. Parts are now also detected by their `.tdpart####of####` document filename in both the search path and the folder listing, and the frontend search filter mirrors the same rule. Only the unified main file appears when searching.
- **No duplicate tooltips**: command bar icons no longer show both the custom tooltip and the native browser tooltip on hover.

## [1.3.1]

### Added

- **Flood-wait countdown**: the Rust backend now emits a `flood-wait` event whenever a transfer enters Telegram FLOOD_WAIT, and a countdown button appears in the icon rail with a live popover showing remaining time, retry attempt, and a note on Telegram's rate limits.
- **Quiet icon tooltips**: every icon action in the rail and command bar now shows a consistent, minimal tooltip on hover instead of the native browser tooltip.
- **Sync status popover**: the rail connection dot opens a popover with live connection state, up/down bandwidth usage against the 250 GB daily budget, and a Sync-folders action.

### Changed

- **Command bar cleanup**: Analytics, Settings, and the theme toggle now live only in the icon rail (no duplicated actions); the search bar moved to the right so long folder names are no longer cropped; the context label reads "Saved Messages" or "Channel" to match the MTProto model.
- **About panel**: credits the author Rasyid Muhamad Muflih Zain; legacy donation links removed.

### Fixed

- **Collections drawer scroll**: the folder list inside the drawer now scrolls correctly instead of being clipped.
- **Rail brand mark**: shows the real TeleStash logo and is no longer clickable (the Collections button is the single entry point).

## [1.3.0]

### Added

- **Vault identity & console layout**: self-hosted Outfit variable font (uniform typography everywhere), charcoal + gold Vault palette with refreshed theme presets, and a new console layout — icon navigation rail, folder drawer, and a compact command bar. Phosphor icons replace Lucide.
- **CI workflow** (`.github/workflows/ci.yml`): every pull request and main push now runs the same validation as releases — `tsc --noEmit`, automated unit tests, and `cargo check --locked` on a Windows MSVC runner.

### Changed

- **Single ingestion path — file upload only**: uploads go through the file picker into the currently selected folder, with the upload entry as the first item in grid and list views. Folder upload (zip-and-upload), remote URL upload, and OS/Explorer drag-and-drop ingestion were removed along with their backend commands (`cmd_zip_folder`, `cmd_upload_from_url`), the unused `reqwest` dependency, and their UI/i18n surfaces. In-app drag-to-folder moves still work.
- **Direct-only networking**: the `grammers-mtsender` `proxy` feature is no longer enabled; no proxy or VPN configuration remains in the app.
- **Dependency updates**: framer-motion 12 → 13 (React 19 strict-mode compatible), bcrypt 0.16 → 0.19, sysinfo 0.30 → 0.39, actix-multipart 0.7 → 0.8, sevenz-rust2 0.7 → 0.22 (with archive API adaptation and upstream malicious-archive hardening), zip 2.4 → 4.6 (deflate-only feature set preserved).

### Removed

- Dead code: unused `DragDropOverlay` component and legacy planning documents; ~700 lines of unreachable backend upload machinery.

## [1.2.4]

### Changed

- **MTProto Core: grammers migrated to crates.io 0.10.0**:
  - The four `grammers-*` dependencies moved from a frozen GitHub rev (Nov 2025) to the published 0.10.0 releases; grammers itself relocated to Codeberg while continuing to publish on crates.io, and Dependabot can now track these crates natively.
  - Adapters across 13 Rust files for the 0.10 API: module split (`types` → `media`/`peer`/`message`), peer identity now flows as session-backed `PeerRef`, `Client` construction via the sender-pool fat handle, async `SqliteSession::open`, `Document::size()`/`name()` returning Options.
  - Closes the last two security alerts: hickory-proto 0.25.2 (NSEC3 unbounded-loop HIGH and O(n²) name-compression MEDIUM) are gone — mtsender 0.10 pulls hickory-resolver 0.26.1.

### Fixed

- **"Restoring session..." hang with grammers 0.10 (critical)**:
  - libsql 0.9.30 asserts `SQLITE_CONFIG_SERIALIZED` can still be set during its global init, but `db.rs` initialized SQLite first in the process, so the config call returned `SQLITE_MISUSE`, the assertion panicked, libsql's internal task died silently and `SqliteSession::open().await` stayed pending forever — session restore hung with no error, even after a clean install and fresh login.
  - Fix: open a throwaway libsql session at the very start of `run()` so libsql's threading config is applied before every other SQLite user in the process.
- **Session restore diagnostics**: restore errors and 30-second hangs now surface as visible toasts (previously `console.warn`-only, invisible in installed builds).

### Improved

- **CI/CD**: `publish-release` is skipped for non-tag workflow dispatches so manual builds end green; release workflow actions bumped to Node 24-native majors (checkout v7, setup-node v7, upload-artifact v7, download-artifact v8, github-script v8), removing the per-job Node.js 20 deprecation annotations.

## [1.2.3]

### Added

- **Live Update Banner (No Restart Required)**:
  - TeleStash now checks for new releases every 30 minutes while the app is open, so the top update ribbon appears live the moment a release is published — no more exit-and-relaunch.
  - A shared updater state connects the ribbon, Settings, and the tray: a manual check from anywhere instantly surfaces the banner.
  - Tray **Check for Updates** now actually performs a check (previously it only opened Settings).
- **Update Dialog with Actions**:
  - Settings "Check Now" now opens a dialog offering **Download Now** (with live progress in the ribbon and Settings) or **Remind Me Later**.
  - **Remind Me Later** snoozes the update banner for 24 hours (`telestash_update_snooze_until` in localStorage); the banner's X button snoozes the same way instead of reappearing on the next check.
  - Localized dialog strings across all 13 supported languages.

### Changed

- **Cinema Low-Power Behavior (Tray Idle)**:
  - While the window is hidden in the tray (e.g., watching via MPV), the webview performs zero scheduled work: the 10-second network status poll and the 30-minute update check skip entirely while `document.hidden`, and the 5-second bandwidth query auto-pauses as before.
  - On restore from tray, network status refreshes instantly and update checks resume (throttled to once per 10 minutes so alt-tabbing stays quiet).
  - The MPV streaming path (loopback HTTP → Telegram MTProto) is untouched.

### Removed

- **Dead Code & Dependency Cleanup (~3,650 lines)**:
  - Removed the unreachable HLS-transcode/fMP4 subsystem (`transcode.rs`, `fmp4_remux.rs`, 12 commands, `/hls` + `/fmp4` routes, `actix-files`, FFmpeg detection, and the related Settings cache UI, file-card badges, and types) — playback has always used the `/stream` loopback path.
  - Removed dead Rust modules (`streaming_buffer`, `upload_checkpoint`, `session_health`, `batch_cc_queue`), two unused SQLite checkpoint tables, three never-invoked commands, and dead frontend hooks/scripts (`useStreamingSettings`, `moovCache`, `useFileDrop`, `check-i18n.cjs`, `sync-keys.cjs`).
  - Uninstalled unused npm packages (`@tauri-apps/plugin-opener`, `plugin-os`, `plugin-deep-link`) and dropped unused crate declarations; legacy `transcodeCacheMaxGb` setting is pruned on load.

## [1.2.2]

### Added

- **Resizable Desktop Sidebar (Adjustable Width)**:
  - Added an interactive drag-to-resize handle on the right edge of the left navigation sidebar.
  - Dynamically adjust sidebar width from `200px` to `520px` to comfortably display long folder names without truncation.
  - Automatically persists user-customized width in `localStorage` (`telestash_sidebar_width`).

### Fixed

- **Cross-Series Episode Isolation in "Next Up" & Parser**:
  - Strictly enforce `seriesTitle` matching in `getNextEpisode` so watching an episode from Series A (e.g. *Midnight Mass S01E01*) never matches an episode from Series B (e.g. *The Mentalist S01E02*).
  - Context-aware *Next Up* bar now prioritizes the series folder currently open on screen.
- **Enhanced MPV External Subtitle Loading in Playlists**:
  - Implemented multi-key caption caching (including video stem filenames) ensuring MPV fuzzy matching auto-loads subtitles across 100% of playlist episodes.
  - Injected explicit `--sub-file` arguments during both single-item and playlist playback.
  - Added Indonesian language codes (`id`, `ind`, `Indonesian`) to `--slang` subtitle auto-selection.
- **Attach Subtitles Modal State Lifecycle & Feedback**:
  - Displays instant visual toast confirmation on success (`Successfully attached N subtitle tracks!`).
  - Automatically clears selection and progress state when upload succeeds or when cancelled, while safely preserving selections if an error occurs.
  - Automatically invalidates React Query cache so subtitle badges update immediately across the UI.
- **Clean Global Search Results**:
  - Filtered out `[telestash-part]` chunk messages, `.tdmanifest.json` files, and `#telestash_sub:` metadata messages from global search results in both backend Rust and frontend UI.

## [1.2.1]

### Added

- **External Subtitles Sidecar & Smart Linkage System**:
  - **Zero-Clutter Series Management**: Subtitle files (`.sub` + `.idx` VobSub, `.srt`, `.ass`, `.ssa`, `.vtt`) uploaded via the sidecar engine are tagged with `#telestash_sub` and automatically hidden from regular file explorer views, keeping the series list 100% clean and pristine.
  - **Smart Subtitle Matcher**: Automatically scans and pairs video filenames (`The.Office.S01E01.mkv`, `9x01.mkv`, `Superfan S08E24.mkv`) with matching subtitle tracks in `sub/` folders or multiple file selections.
  - **Multi-Format & Multi-Language Support**: Supports bitmap VobSub pairs (`.idx` + `.sub`), SubRip (`.srt`), Advanced SubStation Alpha (`.ass`), and WebVTT (`.vtt`) with automatic language detection (English, Indonesian, Japanese, etc.).
  - **Interactive Attach Subtitles Modal**: Added dedicated "Attach Subtitles" button in the Series / Folder toolbar with a live preview matching table before uploading.
  - **Visual Subtitle Badges**: Displays `SUB: EN` / `SUB: ID` badges on video cards and list items.
  - **Seamless MPV Playback**: Automatically downloads/caches attached subtitle sidecars and supplies `--sub-file` arguments during playback. Switch tracks instantly on-the-fly via the `c` key in MPV.
  - **Comprehensive CI/CD Unit Test Suite**: Integrated `npm test` covering `subtitle-matcher`, `series-parser`, `updater-signing-key`, and `versions` verification into the GitHub Actions release workflow.

## [1.2.0]

### Added

- **Granular Per-File Pause & Resume Transfers**:
  - Individual **Pause (⏸️)** and **Resume (▶️)** buttons per file row in the transfer queue popup.
  - **Non-blocking Queue Skipping**: Pausing an active or pending file (e.g. file #7 in a 10-file bulk upload) allows the transfer worker to bypass it and proceed uploading subsequent queued files.
  - **Batch Controls**: Added header **Pause All** and **Resume All** buttons to pause or resume entire queues with one click.
  - **Persistent Cross-Session State**: Paused and pending items are persisted to Tauri Store. If the app is closed or the computer is powered off mid-transfer, reopening TeleStash preserves the exact progress and state so transfers resume without restarting from 0%.
  - **Zero-Resource Backend Suspension**: Async waker-based polling (`Poll::Pending`) in Rust streaming reader ensures paused uploads/downloads consume 0% bandwidth and 0% CPU without dropping stream connections.
  - **Download Checkpoints DB**: Added `download_checkpoints` SQLite table for persistent tracking.

### Fixed

- **Windows System Tray Infinite Dialog Loop**: Removed `Upload`, `Downloads`, and `Resume Video` from the Windows System Tray menu to eliminate repetitive/infinite file picker dialogs, keeping the tray clean and stable (`Open TeleStash`, `Check for Updates`, `Settings`, `Exit`).

## [1.1.9]

### Added

- **Custom MPV Player Keybindings**:
  - `Up` / `Down` & `Mouse Wheel`: Adjust volume (+/- 2%).
  - `Ctrl + Left/Right`: Fast 30-second seek (`seek -30` / `seek +30`).
  - `Shift + Left/Right`: Precise 10-second seek (`seek -10` / `seek +10`).
  - `c`: Cycle / roll subtitle tracks.
  - `Enter`: Toggle fullscreen mode.
  - `Tab`: Display full technical media overlay (codecs, bitrate, fps, dropped frames, audio/sub tracks).
  - `Ctrl + f`: Jump to previous episode / playlist item.
  - `Ctrl + j`: Jump to next episode / playlist item.

### Fixed

- **Smart Re-Post Rename for Forwarded Media (`copy_media`)**: Resolved Telegram MTProto `MESSAGE_ID_INVALID` error when renaming forwarded files (e.g. files moved into a new folder) by seamlessly using `InputMessage::copy_media` to re-send the cloud media reference under the new filename and purge the old forwarded message record.

## [1.1.8]

### Added

- **Automatic English Subtitle Selection in MPV**: MPV now automatically selects and displays embedded/external English subtitle tracks across US & UK regional variations (`--slang=en,eng,enUS,en-US,enGB,en-GB,en-UK,enUK,English,eng-US,eng-GB`), even if not marked as the default track in the media container.
- **Media Badge Resolution Deduplication**: Intelligently prevents duplicated resolution badges (e.g. `1080p` and `1080P`) when a file has both filename resolution tags and container-probed metadata.
- **Multi-Season Flex-Wrap Layout**: Season tabs container in series folders now wraps responsively so Season 9 and beyond are always visible on screen without clipping.

### Fixed

- **Smart Rename Fallback for Forwarded / Moved Files**: Resolved Telegram MTProto `MESSAGE_ID_INVALID` error when renaming forwarded or moved files between channels by automatically re-posting the existing cloud media reference with the new caption and removing the old message record.
- **Universal Episode Delimiter Matching**: Enhanced episode parser regex with non-alphanumeric boundaries to support single quotes, unicode smart quotes, and parentheses around `NxEE` tags (e.g. `'New Guys'`, `(US) - 9x01`).

## [1.1.7]

### Added

- **Dynamic In-Folder Series Progress Tracker**: Dedicated progress strip inside series folders showing last watched episode, 1-click Resume, and instant "Next Up" continuation action that dynamically updates as you switch between different series folders (e.g. *The Office*, *Silo*, etc.).
- **Smart Deduplication for Recent Watch / Continue Watching**: Automatically consolidates multiple episodes of the same TV series or folder into a single latest watch card, removing old intermediate episodes while preserving full logs in the Watch History modal.
- **Calm Minimalist Recent Watch Cards**: Refined dark editorial highlight cards with subtle slate/emerald tone, crisp monospace metadata, and tactile hover feedback compliant with `/design-taste-frontend` and `/minimalist-ui`.
- **Automated Dependabot Security Config**: Added `.github/dependabot.yml` for automated weekly vulnerability auditing across npm and cargo dependencies.

### Fixed

- **MPV Playlist `.m3u8` Architecture**: Replaced command-line argument expansion with temporary `.m3u8` playlist files, completely eliminating Windows `os error 206` ("The filename or extension is too long") when playing or binging large series folders with hundreds of episodes (e.g. *The Mentalist* 7 seasons / 168+ episodes).
- **Universal Episode Regex Parser**: Added multi-pattern support for classic scene `NxEE` notations (e.g. `9x01`, `09x01`), Superfan rip naming conventions, season dashes (`Season 8 - 24`), and multi-episode parts (`S08E24-E25`).
- **Dependency Security**: Upgraded `pdfjs-dist` to `^6.2.108` and resolved all npm audit vulnerabilities (0 vulnerabilities).
- **CI/CD Workflow**: Added quoted environment variable strings to silence GitHub runner Node 20 deprecation notices.

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
