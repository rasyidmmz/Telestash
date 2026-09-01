# TeleStash — Product Requirements Document (PRD)

## 1. Executive Summary & Product Vision

**TeleStash** is a native Windows 11 64-bit desktop application that transforms Telegram into an unlimited, personal cinema cloud and high-performance media vault. Built with **Tauri v2, Rust, React 18, and a bundled MPV sidecar**, TeleStash enables users to store, organize, stream, and subtitle personal collections of high-definition movies and TV series (including 4K UHD, 10-bit HDR, HEVC/H.265, AV1, MKV, and MP4) directly through Telegram MTProto without intermediate proxy servers, third-party storage fees, or transcoding loss.

---

## 2. Target Audience & Core Personas

* **Media Enthusiasts & Cinephiles**: Users who collect high-bitrate 1080p/4K HDR movies with multi-channel surround audio (Dolby Atmos, DTS-HD) and demand native zero-copy playback without browser codec limitations.
* **Binge-Watchers**: Users storing full multi-season TV series who require automated season grouping, seamless sequential playback, and smart "Next Up" tracking.
* **Privacy-Conscious Users**: Users who want direct, end-to-end MTProto communication with Telegram servers using their own API credentials without routing traffic through third-party servers.

---

## 3. Core Architecture & Tech Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    TeleStash UI (React 18)                  │
│       Tailwind CSS v4 · Lucide Icons · TanStack React Query  │
└──────────────────────────────┬──────────────────────────────┘
                               │ Tauri v2 IPC Commands & Events
┌──────────────────────────────▼──────────────────────────────┐
│                    Tauri v2 / Rust Backend                  │
│  ┌───────────────────────┬────────────────────────────────┐ │
│  │ Direct MTProto Client │ SQLite Checkpoint & Cache DB   │ │
│  │ (Grammers Async Core) │ (upload/download checkpoints)  │ │
│  ├───────────────────────┼────────────────────────────────┤ │
│  │ HTTP Stream Server    │ Transfer Manager & Classifier  │ │
│  │ (16 MiB Ring Buffer)  │ (FLOOD_WAIT & retry engine)    │ │
│  └───────────────────────┴────────────────────────────────┘ │
└──────────────┬──────────────────────────────┬───────────────┘
               │                              │
┌──────────────▼──────────────┐┌──────────────▼───────────────┐
│     Native MPV Sidecar      ││    Local Whisper AI Engine   │
│ (HEVC, 4K HDR, Subtitles)   ││   (English CC SRT Generator) │
└─────────────────────────────┘└──────────────────────────────┘
```

### 3.1 Technology Stack
* **Desktop Framework**: Tauri v2 (`@tauri-apps/api`, `@tauri-apps/cli`).
* **Frontend**: React 18, TypeScript 5, Vite, Tailwind CSS v4, Lucide React, `@dnd-kit` (drag & drop).
* **State Management**: TanStack React Query v5, React Context (`SettingsContext`), Tauri Store.
* **Backend**: Rust 2021 Edition (Tokio async runtime, Actix/Tauri IPC).
* **Telegram Protocol**: `grammers-client`, `grammers-tl-types`, `grammers-session`.
* **Database**: Embedded SQLite (rusqlite / sqlite) for transfer checkpoints, download states, and subtitle metadata.
* **Media Engines**: Bundled MPV native binary (x64) and Whisper CLI (`ggml-base.en.bin`).

---

## 4. Functional Specifications & Feature Modules

### 4.1 Personal Cinema Streaming & MPV Integration
* **Direct MTProto Streaming**: Media chunks are fetched on-demand from Telegram Data Centers and streamed to MPV via a local HTTP loopback server.
* **Bounded In-Memory Ring Buffer**: Implements a strict 16 MiB forward memory buffer per active stream to ensure smooth seeking without consuming excessive local disk or RAM.
* **Custom Cinema Keybindings**:
  * `Up` / `Down` & `Mouse Wheel`: Adjust volume (+/- 2%).
  * `Ctrl + Left/Right`: Fast 30-second seek (`seek -30` / `seek +30`).
  * `Shift + Left/Right`: Precise 10-second seek (`seek -10` / `seek +10`).
  * `c`: Cycle / roll subtitle tracks.
  * `Enter`: Toggle fullscreen.
  * `Tab`: Display technical overlay (codecs, bitrate, fps, dropped frames, audio/sub tracks).
  * `Ctrl + f` / `Ctrl + j`: Jump to previous/next episode in playlist.
* **Smart Language Selection**: Automatic audio and subtitle selection prioritizing English (`en`, `eng`, `enUS`, `en-US`) and Indonesian (`id`, `ind`, `Indonesian`) via `--slang` and `--alang`.

### 4.2 TV Series Auto-Grouping & Progression
* **Regex Episode Parsing**: Recognizes standard formats (`S01E05`), multi-season notations (`9x01`), and special editions (`Superfan Season 7 S07e24`).
* **Dynamic Season Tabs**: Automatically groups episodes into responsive tabs (`All Episodes`, `Season 1`, `Season 2` ... `Season 9+`) with natural numeric sorting.
* **Cross-Series Isolation in "Next Up"**: Strictly validates `seriesTitle` matching so watching an episode in *Series A* never contaminates or advances *Series B*.
* **1-Click Binge Series**: Queues and auto-plays all remaining episodes in sequence within MPV.

### 4.3 External Subtitles Sidecar Engine
* **Zero-Clutter Cloud Storage**: Subtitle files (`.srt`, `.ass`, `.vtt`, `.sub` + `.idx` VobSub) uploaded via the sidecar engine are tagged with `#telestash_sub:` and hidden from regular file listings.
* **Smart Subtitle Matcher**: Automatically scans, pairs, and attaches subtitle files from local disk or `sub/` directories to matching video files.
* **Multi-Key Subtitle Caching**: Caches subtitle files locally under both message ID and video filename stem, ensuring 100% reliable auto-loading during sequential playlist playback.

### 4.4 Large-File Split Transfer & Checkpoint Resume
* **2 GB Split Engine**: Files exceeding `2_000_000_000` bytes are automatically divided into 512 MiB chunks (`[telestash-part]`) with a cryptographic `.tdmanifest.json` manifest and presented seamlessly as a single unified file in the UI.
* **Granular Pause & Resume**: Individual and batch Pause/Resume controls in the transfer queue. Pausing suspends I/O via async wakers (`Poll::Pending`) consuming 0% CPU and 0% network bandwidth.
* **Persistent Transfer State**: Checkpoints are stored in SQLite and Tauri Store, allowing transfers to resume seamlessly across application restarts.
* **Flood Wait & Retry Policy**: Respects Telegram MTProto `FLOOD_WAIT_X` sliding window backoff and retries transient network errors with exponential backoff.

### 4.5 Navigation, Sidebar & Search
* **Resizable Left Sidebar**: Interactive drag handle allowing users to resize the navigation sidebar between `200px` and `520px`, persisted in `localStorage` (`telestash_sidebar_width`).
* **Clean Global Search**: Filters out split parts (`[telestash-part]`), manifests (`.tdmanifest.json`), and subtitle metadata messages (`#telestash_sub:`) in both Rust backend and React frontend.

### 4.6 Local Whisper AI Subtitles
* **On-Device Subtitle Generation**: Uses bundled Whisper CLI and `ggml-base.en.bin` model to extract audio and generate `.en.srt` subtitles without external API costs.
* **Resource Throttling**: Operates with Windows `BELOW_NORMAL_PRIORITY_CLASS` and a 2-thread CPU limit to ensure host system responsiveness.

---

## 5. Non-Functional Requirements & Platform Boundaries

| Dimension | Specification |
| :--- | :--- |
| **Supported OS** | **Windows 11 64-bit exclusively** (No macOS, Linux, Android, or iOS targets). |
| **Network Path** | **Direct Telegram MTProto only** (No proxies, VPNs, or network hooks). |
| **Memory Footprint** | Bounded forward buffer (<64 MiB RAM during 4K streaming). |
| **Installer Format** | Windows NSIS Single-File Installer (`.exe`) signed with digital updater key. |
| **Version Sync** | Exact 5-way version parity across `app/package.json`, `app/package-lock.json`, `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, and `CHANGELOG.md`. |

---

## 6. Verification & Quality Gates

* **Automated Unit Tests**: `npm test` runs 4 test suites before any release:
  1. `verify-versions.test.js` — Ensures 5-way version synchronization.
  2. `series-parser.test.js` — Tests series regex, season grouping, and cross-series isolation.
  3. `subtitle-matcher.test.js` — Tests VobSub, SRT pairing, and language detection.
  4. `verify-updater-signing-key.test.js` — Validates NSIS signature key identity.
* **Build Verification**: `npm run build` (`tsc && vite build`) must pass with 0 errors.
* **CI/CD Pipeline**: Release workflow on tag push (`v*`) builds on Windows MSVC runner, validates assets, signs updater artifacts, and publishes to GitHub Releases.
