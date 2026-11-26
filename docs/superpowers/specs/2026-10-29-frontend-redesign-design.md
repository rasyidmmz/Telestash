# TeleStash frontend redesign

Date: 2026-10-29  
Status: design proposal and delivery roadmap; awaiting design approval  
Selected approach: B, library first  
Scope: the complete existing frontend, including the share password page

## 1. Intent and agreed direction

Make TeleStash feel like a distinctive, modern personal media library for streaming from Telegram. The user selected a dense, fast library as the center of the application, with the cinematic treatment concentrated in media details and the playback experience. The user requested a written plan for now and declined browser mockups at this stage.

The visual design should help someone find a file, understand its source, and start watching quickly. File management, uploads, downloads, subtitles, sharing, and recovery remain first-class parts of the product.

This is a redesign of the existing React frontend inside the Windows 11 x64 Tauri application. The word “web” in the request is interpreted as the visual and interaction quality of the web frontend. A separately hosted streaming website would be a different architecture and scope.

Product text and this project document use English. Conversation with the user may remain Indonesian. Proposed details below are design recommendations, not additional choices already approved by the user.

## 2. Approaches considered

| Approach | Shape | Tradeoff | Decision |
| --- | --- | --- | --- |
| A: cinema first | Artwork-led home, large viewing rows, management reached through secondary navigation | Strong immersion; fewer files and management controls visible at once | Considered |
| B: library first | Persistent library navigation, compact browsing, direct actions, richer media detail | Fast management and predictable scanning; cinematic expression must come from media and detail rather than a large home banner | Selected by the user |
| C: cinema and vault | Viewing home alongside a separate management workspace | Accommodates both jobs; adds destinations and context switching | Considered |

Only B is the basis of this proposal. Prototype variations, if requested later, must all solve the library-first brief.

## 3. Current frontend and evidence limits

The current source was inspected for planning. No live Telegram account session, installed application, rendered frontend, or installer was exercised in this task. This is a source-grounded proposal; actual visual and performance baselines are an implementation prerequisite.

Verified in the current checkout:

- React 19, TypeScript, Vite, Tailwind 4, Phosphor icons, Framer Motion, React Query, and TanStack Virtual are already installed. Outfit is bundled locally.
- `DesktopDashboard.tsx` composes an icon rail, a collections drawer, top bar, recent watching, virtualized file explorer, transfer queues, previews, and dialogs.
- `FileExplorer.tsx` supports list, grid, and poster modes, sorting, favorites, tags, and series/season organization. View mode is a settings value; sort preferences are loaded per folder.
- The initial window is 1200 × 800; the configured minimum is 1000 × 700. These are the important layout constraints for this Windows application.
- `MediaPlayer.tsx` launches native MPV. It is not an embedded browser video player. A successful launch command does not prove a live playback position or that playback remains active.
- Existing media metadata includes duration, codec, audio presence, track count, and dimensions. The inspected model does not provide an external movie catalog, cast, ratings, or synopsis.
- Light, dark, built-in custom themes, and user-created theme palettes are implemented through the existing theme engine.
- The password form for a shared file is inline HTML in `app/src-tauri/src/share_routes.rs`. It belongs in the frontend coverage even though its source is Rust.
- `VaultRail.tsx` currently compares transferred bytes against a hardcoded 250 GB figure. The replacement must show the actual metric and period, without presenting that constant as Telegram storage capacity.
- `TopBar.tsx` displays a command-symbol search glyph, while the shortcut hook supports Ctrl+F on Windows. The redesign should show the actual supported Windows shortcut.
- Existing Vitest/Testing Library files cover areas including transfers, queue recovery, backup, error humanization, and security configuration. `docs/AI_HANDOFF.md` contains older React/testing descriptions; source and package scripts take precedence for this plan.

## 4. Reference direction

These sources inform the proposal. They are examples of information hierarchy, not a requirement to add their services or copy their branding.

| Reference | Useful pattern | TeleStash application |
| --- | --- | --- |
| [Plex interface overview](https://support.plex.tv/articles/200484203-interface-overview/) | Visible library sources, a useful action bar, direct play, and contextual item actions | Labeled source navigation and a stable search/selection toolbar |
| [Plex library view](https://support.plex.tv/articles/200392126-using-the-library-view/) | Browsing controls and media-oriented library presentation | Deliberate list, grid, and poster choices |
| [Infuse](https://firecore.com/infuse) | Artwork, collections, readable media identity, and a library organized around owned media | Thumbnail-led rows and a composed detail surface using available TeleStash data |
| [Apple TV library on Windows](https://support.apple.com/en-nz/guide/tvapp-windows/atvbe9ddc3f/windows) | Clear sidebar categories and direct playback of personal videos | Windows-friendly navigation and a straightforward path to watch |

Reference access: Infuse's official page was opened. Plex and Apple library information was available in search results; direct opening of the selected overview/library URLs returned 403/404 respectively in this run. Visual screenshots of these products were not inspected. Confirm their current rendered appearance when preparing visual prototypes.

### How the requested skills contribute

- **Superpowers brainstorming:** record the chosen approach, specify the complete design, and review it before product implementation. A detailed engineering execution plan follows design approval.
- **Matt Pocock prototype:** when visual exploration is authorized, compare three structurally different library layouts in the existing dashboard context. Keep them temporary, read-only, and clearly separate from the production result. No prototype is requested for this written-only stage.
- **Matt Pocock codebase design:** retain the existing interfaces to media and transfer behavior. Extract shared presentation only where several real consumers need it.
- **Taste Skill:** use its app-screen workflow rather than its marketing-page workflow. The dark app draw offered `true-black-flat` and `data-terminal`; the proposed foundation is `true-black-flat`, seed `908560645`. Its main idea is elevation through neutral lightness, with emphasis belonging to the content.
- **UI/UX Pro Max:** the initial design-system query matched Social Media App and was rejected. One narrower retry matched Video Streaming/OTT. Its dark contrast, continue-watching, and legibility guidance fits; its marketing hero, extra font, indigo palette, and dark-only recommendation do not fit this product's chosen layout or existing themes. React guidance supports retaining virtualization; UX guidance supports visible keyboard focus.
- **Jev:** the bounded comparison recommended the neutral media-library foundation over a monitoring terminal or an OTT marketing hero. This is advisory and does not establish usability or user approval.

The raw generated sheets are exploratory inputs, not the design authority. This document resolves their mismatches with the user's selection and repository constraints. No generated system was persisted as an approved master.

## 5. Visual system proposal

### Identity

Use the existing TeleStash mark and bundled Outfit font, stronger alignment, a compact rhythm, neutral surfaces, and restrained amber media progress. The signature element is a library row that keeps the media thumbnail, title, Telegram source, available technical metadata, and viewing progress readable together. Actual media imagery supplies the richness.

Keep the default dark presentation neutral. Sidebar and main canvas share the same ground. Distinguish panels through lightness; floating menus and dialogs may use a restrained shadow. Selected navigation has brighter text and a neutral selected fill where needed for recognition. Its selected state must also be programmatic.

### Proposed roles and dimensions

| Role | Starting proposal | Validation |
| --- | --- | --- |
| Dark ground / surface / raised | `#000000` / `#111111` / `#2a2a2a` | Verify on real media, dialogs, and the installed WebView |
| Primary / secondary / metadata text | `#ffffff` / `#b4b4b4` / `#949494` | Measure contrast on every surface actually used |
| Media progress and brand hue | Existing amber `#e8a33d` | Use for progress and small brand marks; warnings carry explicit text and their own semantic pair |
| Primary action | Inverted neutral foreground/background | Derive from theme roles; avoid literal white/black assumptions for custom themes |
| Light presentation | Neutral light ground, white surface, dark text | Apply the same hierarchy; validate independently |
| Typography | Outfit; 12 / 14 / 16 / 20 / 24 / 30 px | 14 px dense rows with 20 px line height, 16 px forms; 12 px secondary metadata |
| Numbers | Tabular figures for sizes, speeds, time, and progress | Confirm the bundled face produces aligned digits |
| Layout | 224 px navigation, 56 px toolbar, 24 px outer gutters, 16 px section gaps | Adjust only after checking 1000 × 700 and 1200 × 800 |
| Dense row | About 56 px including a fixed thumbnail slot | Keep labels and row actions legible; do not trade focus visibility for density |
| Corners | 8 px controls, 12 px media/detail containers, round progress ends | Use a small fixed set of roles |
| Motion | Approximately 100–150 ms press feedback; 160–220 ms overlay transitions | Reduced motion removes displacement; no staggered row entrances |

The density, control geometry, and amber substitution intentionally adapt the medium-density Taste sheet to the selected dense library and existing brand. The sheet's generic analytics-heavy detail blueprint is replaced by actual media content. There is no new font or animation dependency.

Keep semantic tokens compatible with `--color-stash-*`. Add only the few roles required by real consumers; derive them from existing palette values where possible. Preserve theme IDs, saved palettes, active-theme preference, and import/export compatibility. Existing user colors may differ from the bundled visual direction; the redesign must not silently rewrite them. Record any contrast limitations found in representative custom-theme checks.

### Content and imagery

Use existing file thumbnails/previews and technical metadata. Reserve image space before it loads. A missing image gets a composed file-type fallback with its title and real source; a missing duration stays unavailable rather than becoming zero. Poster mode may frame a real preview, but must not imply movie-poster metadata exists. Do not add remote font, artwork, or metadata requests to make the layout work.

## 6. Navigation and interaction model

The application opens into the library. Preserve the saved view mode; use the compact list as the default for a fresh profile. Start in the existing/default folder context, and retain existing context restoration where available. Do not create an “all files” destination that implies a cross-channel index the current data source cannot supply.

The persistent left navigation has the TeleStash identity, Library/source folders, Favorites, Transfers, and Watch history. Existing folder groups and their ordering remain available. Analytics and Settings live in the utility area. Connected account/source context stays legible, without exposing credentials or stream tokens.

Search belongs in the stable top bar. Show the active scope and the actual supported Ctrl+F shortcut. Keep existing search semantics and the current minimum query length; show “Type at least 3 characters” when relevant. View mode becomes an explicit three-choice control rather than a cycling icon. Sort direction, tag/favorite filters, and selection count remain discoverable.

Single click keeps its current file-selection meaning. Ctrl+click toggles selection and Shift+click selects a range. A visible Play/Preview action performs the existing open behavior; Details opens a read-only information surface through a labeled action or context menu. Preserve existing Enter behavior and the F2, Delete, Ctrl+A, Ctrl+D, Ctrl+Shift+S, and Escape contracts where applicable. Nested actions must not also trigger selection, drag, or playback. Folder activation keeps its existing navigation meaning.

When files are selected, show a stable contextual action row with the count, Move, Download, Share, Delete, and Clear selection. Name the objects and consequences in destructive confirmations. Returning from details, a dialog, or another destination restores the previous library context, scroll, and meaningful focus. Where current navigation state does not retain these, implement only the state needed for this behavior rather than adding a routing framework.

The collection tree is visible at supported wide sizes. At the configured minimum width, collapse secondary information and open detail as an overlay before sacrificing filename space. The app remains usable at 1000 × 700; the Tauri window minimum stays unchanged. Browser resizing is a layout test, not an additional mobile-platform deliverable.

## 7. Whole-frontend coverage

Unless a longer path is shown, dashboard component filenames below refer to `app/src/components/desktop/dashboard/`. Other frontend paths are under `app/src/`; `app/src/App.tsx`, `DesktopDashboard.tsx` under `components/desktop/`, and the Rust-hosted form are named explicitly. These are current entry points, not mandatory new abstractions.

| Surface and main question | Redesign outcome | Existing code and contracts to retain |
| --- | --- | --- |
| Startup and session recovery: “Can I enter my library?” | Branded, quiet loading; clear delayed connection and recovery feedback | `app/src/App.tsx`, session checks, providers, lazy dashboard, error boundary |
| Sign-in: “What is my next step?” | Readable API setup, QR/phone/code/password steps, field-level errors, paste/password-manager support | `components/shared/AuthWizard.tsx`; existing authentication and support steps |
| Navigation and collections: “Where are my files?” | Persistent labeled folders/groups, clear active source, contextual folder actions | `VaultRail.tsx`, `Sidebar.tsx`, `SidebarItem.tsx`; create/rename/delete, drag/reorder, visibility, invitation and group operations |
| Command bar and search: “What am I browsing?” | Scope, search, view mode, selection actions, concise feedback | `TopBar.tsx`, dashboard search callbacks and existing shortcut hook |
| Library: “Which file do I need?” | Dense list first, coherent grid/posters, technical data subordinate to names, stable sort/filter controls | `FileExplorer.tsx`, `FileListItem.tsx`, `FileCard.tsx`; virtualization, saved preferences, favorites/tags and season logic |
| Continue watching: “Where did I leave off?” | Compact strip above the library, with Resume and a real next-episode action when available | `RecentWatchBar.tsx`, `utils/watchHistory.ts`, `utils/seriesParser.ts`; folder/message identity and stored resume position |
| Media detail: “What is this, and can I play it?” | Artwork/preview-led detail; title, source, real metadata, subtitles, progress and direct actions | Existing preview, metadata, favorites/tags, sharing and subtitle hooks; a small presentation surface only if needed |
| Playback handoff: “Did the player open?” | Coherent preparing/opened/error states; retry/open action and return to library | `MediaPlayer.tsx`; stream URL identity, MPV launch, playlist order, native resume/subtitle behavior |
| Other previews: “Can I inspect this file?” | Consistent frame, title, close/next/previous and download actions | `PreviewModal.tsx`, `PdfViewer.tsx`; image, document, archive and other existing preview behavior |
| Transfers: “What is moving, or needs attention?” | One Transfers destination with Uploads/Downloads sections and an unobtrusive active summary | `UploadQueue.tsx`, `DownloadQueue.tsx`, `BandwidthWidget.tsx`, existing upload/download/flood-wait hooks and recovery |
| File and folder actions: “What will this action change?” | Consistent names, focus, validation and confirmation hierarchy | `ContextMenu.tsx`, rename/move dialogs, `TagEditorModal.tsx`, bulk-link UI in `DesktopDashboard.tsx`, `ConfirmContext.tsx` |
| Subtitles: “Which subtitles will this file use?” | Clear file matching, language/label, pairing, selection, upload and removal feedback | `AttachSubtitlesModal.tsx`, `ManageSubtitlesModal.tsx`, `useVideoSubtitles.ts`, existing pairing/matching rules |
| Sharing: “Who can access this file?” | Link, password, expiry, copy, revoke and public/private wording remain explicit | `ShareDialog.tsx`, existing sharing settings, backend authorization and escaping |
| Shared-file password form: “How do I open this link?” | Matching lightweight colors/type roles, visible password label, inline failure and keyboard focus | HTML in `app/src-tauri/src/share_routes.rs`; keep form method/action, token handling, HTML escaping, password verification and existing route behavior |
| Settings: “Where is this preference?” | Roomier settings workspace with named sections; one content area at a time; existing save behavior explained consistently | `SettingsModal.tsx`, `SettingsContext.tsx`; General, Playback, Themes, Sharing, About and their existing options |
| Theme editing: “How will my choice look?” | Live representative controls, consistent token roles, readable light/dark presentation | `context/ThemeContext.tsx`, `theme/themeEngine.ts`, `theme/presets.ts`, existing custom-theme UI and persistence |
| Backup and restore: “What will be exported or replaced?” | Keep scope/consequences adjacent to actions, clear busy/result states | `backup.ts`, `backup.test.ts`, existing settings backup UI and schema |
| History, analytics, diagnostics: “What happened?” | Readable tables and summaries, factual units/time ranges, filtering/actions appropriate to existing capabilities | `WatchLogsModal.tsx`, `WatchAnalyticsModal.tsx`, `StorageAnalyticsModal.tsx`, `LogsModal.tsx`, `errorLogs.ts` |
| Updates and common feedback: “What needs my attention?” | Consistent update progress, inline errors, toast hierarchy, pending and disabled states | `UpdateBanner.tsx`, `UpdateContext.tsx`, `ErrorBoundary.tsx`, `EmptyState.tsx`, badges, file icons and toast consumers |

The coverage includes all UI states in these surfaces: initial loading, background refresh, empty library, empty search, unavailable thumbnail/metadata, disconnected account, protocol waiting, transfer failure, permissions, invalid input, and disabled controls. Preserve privacy and error redaction when presenting diagnostics.

### Detail and playback boundaries

Media detail carries the cinematic expression through a large genuine preview, strong title, generous local spacing, and a clear Play/Resume action. The library itself keeps its compact rhythm. A detail panel becomes an overlay on smaller supported windows so it cannot compress file names into unreadability.

Keep native MPV as the playback engine. The React handoff may say “Opening player” and “Opened in player” when those states are supported. A seek bar, volume control, live buffering graph, or embedded video surface would require a separate playback-control design and verified backend support. This proposal does not assume such support exists. Native MPV chrome/OSC replacement is outside this frontend restyle.

### Transfer truth and continuity

The transfer destination reuses the existing hooks and separate queue models. Navigation must not unmount their owner or cancel work. Reopening Transfers shows the same jobs. Pause, resume, retry, cancel, clear-finished, individual and bulk controls retain their current semantics and availability. The summary counts active/actionable upload and download jobs explicitly rather than labeling both as uploads.

Show bytes, measured speed, progress when known, and a protocol wait countdown when provided. Do not fabricate storage capacity, ETA, or completion percentages. Preserve split uploads above `2_000_000_000` bytes, permanent `FILE_PARTS_INVALID` classification, and Telegram `FLOOD_WAIT` backoff. No proxy or bandwidth-throttle settings are introduced.

## 8. Accessibility, performance, and quality targets

These are acceptance targets to test later, not results measured in this planning task.

- Normal text contrast at least 4.5:1; large text and meaningful control/icon boundaries at least 3:1. Decorative dividers do not substitute for identifiable interactive controls.
- Visible focus, correct names and selected/expanded states, logical traversal, and keyboard alternatives for dragging. Reuse and test `useModalDialog` for actual modal surfaces, including focus return. A nonmodal detail panel must not trap focus.
- Desktop toolbar targets should be comfortable at about 36–40 px, with spacing and a larger effective target where feasible. Primary form actions are about 40–44 px. Keep dense rows usable under Windows scaling and text enlargement; do not interpret mobile-specific skill guidance as a new platform requirement.
- Verify Windows keyboard operation, reduced motion, 100%/125%/150% display scaling, long file names, supported localized labels, and RTL where already supported. New strings use existing translation keys/fallback conventions; English is the source default.
- Preserve virtualization in every library layout. Thumbnail or metadata failure must not prevent selection, search, transfer, or playback actions. Do not fan out metadata requests for every offscreen file.
- Compare the same 1,000-file and 10,000-file fixtures before and after: scroll, selection, filter changes, thumbnail activity, and render/interaction timings. Virtualized DOM work must remain bounded by visible content plus overscan. Establish numerical budgets from a recorded baseline before accepting the implementation.
- Reserve thumbnail/card dimensions; keep stable geometry during background refresh and transfer updates. In-flight network results must not replace the current source with an older folder/search response.
- At 1000 × 700, 1200 × 800, 1440 × 900, and 1920 × 1080, filenames, primary actions and focus remain usable without unintended page-level horizontal scrolling. Verify light and dark independently and representative existing custom themes.
- Cross-folder favorites, history, details and playback retain the correct folder/message identity. Equal message IDs in different channels must not play, mutate, or display the wrong file.

## 9. Delivery roadmap and dependencies

Each phase is a reviewable slice. Existing behavior stays available until its replacement passes its checks. The sequence is deliberate: foundations and browsing must settle before the many supporting dialogs adopt the system.

| Phase | Work and deliverables | Exit condition |
| --- | --- | --- |
| 0: baseline and coverage | Capture existing Windows screens, interactions and timings; record the surface/state checklist; confirm data fields and current theme/settings persistence | Coverage matrix and reproducible baseline exist; all unverified assumptions are recorded |
| 1: approved visual contract | Resolve this proposal; document tokens, navigation, density, interaction rules and representative library/detail/transfer/settings layouts | User approves the written design; optional later prototypes answer layout questions, not backend behavior |
| 2: foundation and shell | Normalize existing tokens; update shared controls/focus/dialog patterns; replace hidden primary folder access with labeled navigation and rebuild top bar | Minimum-window layout, theme persistence and primary navigation work without losing active transfers |
| 3: library and search | Restyle list/grid/posters and filters; integrate compact continue-watching; preserve sorting, series, tags/favorites, selection and drag/drop | Browse/search/select flows pass with mixed files, large lists, empty/error states and saved preferences |
| 4: detail and viewing | Add the composed read-only detail surface as needed; update playback handoff, PDF/general previews and subtitle dialogs | Correct-file resume, playlist/subtitle behavior and return-to-library state are verified with MPV |
| 5: transfers and management | Consolidate queue presentation, actual status/bytes/waits, file/folder action dialogs, sharing and the shared password form | Navigating does not interrupt jobs; retries/confirmations/sharing retain their contracts; Rust-hosted HTML is validated in CI when changed |
| 6: settings and supporting surfaces | Apply the design to settings/themes/backup, history/analytics/logs, authentication, session restoration, updates and error feedback | Every inventory row and required state has the new visual system and functional checks |
| 7: acceptance and documentation | Full theme/keyboard/scaling pass, visual comparison, focused regressions, performance comparison, README and relevant architecture updates | Evidence supports completion; known limitations are named; release preparation is a separate authorized action |

Dependencies: 0 → 1 → 2 → 3 → 4. Phase 5 depends on 2 and the library action contracts from 3. Phase 6 uses the same accepted shell/control foundation. Phase 7 follows all surfaces. Do not split the work into a broad rewrite of hooks, the player, and the backend.

After the design is approved, the Superpowers writing-plans step should turn these phases into exact file edits, meaningful tests, and small implementation checkpoints. No calendar estimate is offered before the baseline and first migrated screen establish the actual cost.

## 10. Engineering constraints and validation plan

Reuse existing components, hooks, contexts, query keys, models, confirmation handling, and installed libraries. Create a shared UI primitive only when existing consumers need the same behavior. Keep visual wrappers from owning business state. Do not introduce a new router, state manager, component kit, external artwork provider, or font service for this redesign.

Preserve settings/backup schema, theme storage, session restoration, cache ownership, file IDs, share authorization, queue recovery and public/private semantics. The share form's styling touches Rust source; keep that change confined to presentation and retain escaping and verification exactly.

### Checks during implementation

| Check | Where and purpose |
| --- | --- |
| `npx tsc --noEmit --pretty false` | Run from `app/` after frontend changes |
| `npm test` | Existing Node and Vitest suites; protect transfer, backup/recovery, parser, signer/version and security behavior |
| `npm run build` | Frontend-only TypeScript/Vite build and asset validation |
| Focused interaction tests | Selection versus nested Play/Details actions; focus/close/return behavior; explicit view choice; transfer continuity during navigation; same ID in different folders |
| Backup/theme compatibility tests | Exercise old valid data and representative saved themes in isolated test storage; never overwrite a real profile for a check |
| Visual/state review | Real browser for frontend layouts; installed Windows WebView for native dialogs, drag/drop, MPV handoff and scaling; screenshots with matching fixture data |
| CI compilation | Rust/installer checks exclusively through GitHub Actions when separately authorized, including any change to Rust-hosted HTML |
| `git diff --check` | Whitespace validation before a requested commit |
| Jev final review | Review actual final changes with actual test evidence; a score alone does not establish completion |

Tests should cover changed behavior and failure risks, rather than mirror CSS or assert every element exists. No local Rust compilation or toolchain installation is permitted. Commit, push, tag, workflow triggering, and publication each remain subject to the repository's explicit-approval rule.

Settings restore was reported untested by the user in earlier testing. This plan includes isolated compatibility checks and a future explicit smoke-test checklist; it does not declare restore verified or authorize replacing the user's live settings.

## 11. Completion definition and builder decisions

The redesign is complete when every inventory surface has its new presentation, core browse/watch/manage journeys work in the real Windows application, theme and saved-data compatibility hold, and the validation evidence is available. A polished library screen alone is not completion of this scope.

The builder may choose exact spacing within the proposed scale, small component extraction locations, fixture names, and the smallest internal navigation state implementation. These choices must preserve the agreed interaction and persistence contracts. The user reviews the visual identity and navigation as a whole before product implementation begins.

Explicitly excluded: external metadata/catalog integration, recommendation algorithms, a hosted streaming service, new operating-system targets, playback-engine replacement, protocol changes, a new theme storage format, and release publication. They are separate features and are unnecessary to achieve this redesign.

No product source, dependency, settings, credentials, or release state was changed while preparing this proposal. No frontend tests, browser visual checks, Windows runtime checks, or builds were run for this document-only task.

### Planning review record

The independent Superpowers builder check returned “nothing important left”; remaining layout choices are explicitly left to the builder. Jev verified all three planning-completion claims (confidence 0.97–0.98), but its overall gate escalated: `safe_to_apply` 0.47 and blast-radius confidence 0.46. Its output supplied no concrete defect; this is not an automatic pass. The primary agent rechecked the document against the selected library-first scope, current source interfaces, and the working tree, which contains only this new proposal. The result is suitable for design review; product implementation and runtime acceptance remain pending.
