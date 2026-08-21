<div align="center">

![TeleStash Hero Banner](docs/assets/telestash_hero_banner.jpg)

# TeleStash

### *Windows 11 Personal Cinema Cloud & Media Vault*

[![Release](https://img.shields.io/github/v/release/rasyidmmz/Telestash?style=for-the-badge&color=06B6D4&labelColor=0F172A)](https://github.com/rasyidmmz/Telestash/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows_11_x64-0284C7?style=for-the-badge&logo=windows&labelColor=0F172A)](https://github.com/rasyidmmz/Telestash/releases/latest)
[![Codecs](https://img.shields.io/badge/Codecs-HEVC%2Fx265_%7C_4K_MKV_%7C_10--bit-E11D48?style=for-the-badge&labelColor=0F172A)](https://mpv.io)
[![Engine](https://img.shields.io/badge/Video_Engine-MPV_Native_Sidecar-8B5CF6?style=for-the-badge&labelColor=0F172A)](https://mpv.io)
[![Connection](https://img.shields.io/badge/Connection-Direct_Telegram_MTProto-10B981?style=for-the-badge&labelColor=0F172A)](https://core.telegram.org/mtproto)

<br/>

**TeleStash** is a native Windows 11 desktop application that organizes a personal media library in Telegram-backed folders and opens compatible video through a bundled MPV sidecar. It does not impose an application-level storage quota; available storage and file limits remain governed by the connected Telegram account and Telegram's service rules.

Built with **Tauri v2, Rust, React, and an MPV sidecar**, TeleStash supports personal **HEVC/H.265, 10-bit HDR, MKV, and MP4** libraries through a direct Telegram connection, local Whisper English CC generation, persisted upload state, and detailed transfer diagnostics.

[**Download Latest Release**](https://github.com/rasyidmmz/Telestash/releases/latest) · [**Why TeleStash**](#-why-telestash) · [**System Architecture**](#-system-architecture) · [**Brand Identity**](#-brand-identity--visual-system) · [**Build Instructions**](#-build-from-source)

</div>

---

## ❓ What is TeleStash?

**TeleStash** is a dedicated Windows 11 personal media and file-management application that connects directly to Telegram using MTProto. It provides a personal-library workflow without hosting a separate streaming server or using an application proxy or VPN. Telegram account capacity and service rules still apply.

Unlike a browser-only media workflow, TeleStash is a native 64-bit Rust/Tauri application integrated with a bundled **MPV media engine**. MPV handles compatible media formats, including HEVC/H.265, 10-bit HDR, MKV, and MP4; hardware decoding availability depends on the local MPV and Windows graphics-driver configuration.

---

## 💡 Why TeleStash?

| Feature / Metric | Standard Web Browsers & Cloud Apps | TeleStash Media Vault Engine |
| :--- | :--- | :--- |
| **Codec Support** | Browser support varies by codec and container | **Native MPV Engine**: MP4 and MKV, including HEVC/H.265 libraries when locally supported |
| **Disk Consumption** | May create browser caches | **Bounded media prefetch**: 16 MiB in-memory forward buffer per stream; app state, logs, and subtitle cache remain local |
| **Binge-Watching** | Reopens player window per episode | **Native MPV Playlist**: Automatic episode-to-episode auto-play |
| **Subtitle Generation** | Manual download and sync may be required | **Local Whisper English CC**: generate, cache, and upload SRT files to the active folder |
| **Upload Transfer** | Retry behavior varies | **Transfer integrity**: SQLite resumable checkpoints and validated large-file manifests |
| **Connection Path** | Often routed through a provider service | **Direct Telegram MTProto**: no application proxy or VPN route |

### 🛡️ 1. Absolute Privacy & Security
* **Direct MTProto Connection**: TeleStash connects from the Windows application to Telegram without adding an application proxy or VPN route.
* **User-Owned Credentials**: Authenticate securely using your own Telegram API ID and API Hash ([my.telegram.org](https://my.telegram.org)).
* **Personal Library Boundary**: TeleStash is designed for a user-owned Telegram media library. Telegram's service rules and applicable law remain the governing limits.

### 🎬 2. Personal Cinema Experience
* **Native MPV Cinema Engine**: Open compatible HEVC/H.265, 10-bit HDR, MKV, MP4, and multi-channel-audio media through MPV.
* **MPV Native Playlist Auto-Play**: Automatically queues remaining episodes in a folder so TV series play seamlessly from episode to episode.
* **16 MiB In-Memory Ring Buffer**: Keeps a bounded forward media prefetch buffer per active stream.
* **Explicit Local State**: Streaming media is prefetched in memory, while the application intentionally keeps local settings, logs, upload resume state, and subtitle cache.

### ⏳ 3. Resumable Upload Checkpoints & Split Engine
* **SQLite Resumable Uploads**: Tracks upload progress in the local `upload_checkpoints` database so an interrupted eligible transfer can continue from its saved state.
* **Automatic Large-File Splitting**: Files larger than `2_000_000_000` bytes are split into 512 MiB part messages with a validated `.tdmanifest.json` manifest and presented as one file in TeleStash.

### 🎙️ 4. Automated Whisper AI Subtitles
* **Local Whisper Processing**: TeleStash uses the bundled Whisper CLI to generate English CC SRT files from compatible media.
* **System-Friendly Priority**: Whisper runs with Windows `BELOW_NORMAL_PRIORITY_CLASS` and a maximum of two CPU threads.
* **Cloud Library Reuse**: Generated `.en.srt` subtitle files can be uploaded to the active Telegram folder for later playback.

---

## 🏗️ System Architecture

The data path is intentionally short and deterministic: the Windows application connects directly to Telegram MTProto servers, MPV receives an authenticated local HTTP stream, and large split media retains validation metadata alongside its Telegram message parts.

![TeleStash System Architecture](docs/assets/telestash_architecture.png)

### Core Architectural Layers:
1. **Tauri v2 + Rust Core**: Manages high-performance native process execution, IPC command routing, system tray integration, and SQLite checkpoint state.
2. **Direct MTProto Engine**: Multithreaded Grammers client communicating directly with Telegram cloud infrastructure without intermediate proxy servers.
3. **Bundled MPV Sidecar Engine**: Zero-copy 4K/10-bit HDR video rendering, multi-channel surround audio, embedded subtitle selection, and natural episode playlist auto-play.
4. **Local Whisper AI Engine**: System-friendly `whisper-cli` background runner generating `.en.srt` subtitles with `BELOW_NORMAL_PRIORITY_CLASS` and 2-thread CPU cap.

---

## 🚀 Key Features

* 🎬 **Smart Series & Season Auto-Grouping**: Automatic episode detection (`S01E01`, `EP 02`, anime formats) with season tab filtering (`All Episodes`, `Season 1`, `Season 2`) and natural numeric sorting.
* ⏭️ **"Next Up" Progression Banner**: Instant 1-click continuation card in Continue Watching for the next sequential episode of your TV series.
* 🍿 **"Binge Series / Play All" Mode**: 1-click header button to queue and play an entire season continuously from episode 1 in MPV.
* 🏷️ **Auto-Badge Format & Media Tagging**: Automatic filename analysis displaying micro-badges for `4K UHD`, `1080p`, `HDR`, `10-bit`, `HEVC`, `AV1`, `Dual Audio`, and `Atmos`.
* 🎥 **Native MPV Cinema Engine**: Playback for compatible HEVC/H.265, 10-bit HDR, MKV, MP4, and surround-audio media.
* 🔔 **Windows 11 System Tray**: Background minimization with quick slim native context menu (Resume Video, Downloads, Upload, Check Updates, Settings, Open, Exit).
* 📊 **Folder Storage Analytics Dashboard**: Visual distribution charts for Video, Audio, Subtitle, and Document space allocation.
* 🍿 **Recent Watch Bar & Dedicated Watch Logs**: Continue watching strip with instant playback resume and separate activity logs.
* ⚡ **Direct Telegram Transfer Engine**: Shared retry classification, protocol backoff, and diagnostic logs for uploads and downloads.
* 💾 **SQLite Resumable Uploads**: Persisted checkpoints support eligible interrupted transfers.
* 🎙️ **Automated Whisper AI Subtitles**: Local English CC generation, SRT caching, and active-folder upload.
* 🛡️ **Bounded Streaming Prefetch**: 16 MiB in-memory forward buffer per active stream; no full media download is retained by the stream path.
* 📁 **Folder & Channel Storage**: Organize movies and TV series using Saved Messages and private channels as folders.
* 📊 **Transfer Diagnostics**: Detailed error classification, attempt history, part indexes, and retry decisions in the desktop logs view.
* 🖥️ **Windows 11 System Integration**: Autostart toggle via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

---

## 🎨 Brand Identity & Visual System

### Core Brand Metaphor: *The Vault & The Stream*
TeleStash fuses **The Vault** (a personal, immutable Telegram-backed library) with **The Stream** (instant local MPV playback from an in-memory ring buffer). The visual identity balances high-precision developer tooling with a dark, cinematic media experience.

### 🎨 Color System Palette

| Color Name | Hex Code | HSL Value | Primary Application |
| :--- | :--- | :--- | :--- |
| **Electric Cyan** | `#06B6D4` | `hsl(189, 94%, 43%)` | Primary brand accent, MTProto active stream indicators, system focus states |
| **Midnight Slate** | `#0F172A` | `hsl(222, 47%, 11%)` | Cinema viewport canvas, dark-mode root desktop background |
| **Deep Charcoal** | `#1E293B` | `hsl(215, 25%, 27%)` | Card containers, sidebar navigation background, dialog panels |
| **Vivid Emerald** | `#10B981` | `hsl(160, 84%, 39%)` | Active connection badge, SQLite upload checkpoints, successful transfers |
| **Crimson Rose** | `#E11D48` | `hsl(347, 77%, 50%)` | Codec engine tags, error classification, emergency cancellation controls |

### 🔤 Typography System
* **Display & Interface**: `Geist` / `Inter` — Geometric sans-serif engineered for high legibility across Windows desktop displays, file cards, and player controls.
* **Monospace & Telemetry**: `Geist Mono` / `JetBrains Mono` — Monospaced typography for file paths, transfer rates, message IDs, and system diagnostic logs.

### 📐 Design Principles
1. **Zero-Slop Transparency**: Direct MTProto connection without hidden proxy hops or application-level data collection.
2. **Distraction-Free Cinema**: Dark UI surfaces step back so media content and playback controls take center stage.
3. **Deterministic Performance**: In-memory ring buffer, bounded prefetching, and predictable CPU/RAM allocation.

---

## 💻 System Requirements

| Specification | Minimum Requirement |
| :--- | :--- |
| **Operating System** | Windows 11 (64-bit) |
| **Processor** | 64-bit Dual-Core CPU |
| **Memory** | 4 GB RAM |
| **Graphics** | DirectX 11 compatible GPU |
| **Network** | Broadband Internet Connection |

---

## ⬇️ Installation

Download the official setup installer from the [Releases](https://github.com/rasyidmmz/Telestash/releases/latest) page:

```text
TeleStash_<version>_x64-setup.exe
```

1. Run `TeleStash_<version>_x64-setup.exe` and follow the prompt.
2. Sign in with your Telegram API Credentials (API ID & API Hash from [my.telegram.org](https://my.telegram.org)).
3. Open your personal media library.

---

## 🛠️ Build From Source

### Prerequisites
- Node.js (v18+)
- Rust (Stable Toolchain)
- Visual Studio Build Tools with **Desktop development with C++**
- WebView2 Runtime

### Setup & Run
```powershell
# Clone the repository
git clone https://github.com/rasyidmmz/Telestash.git
cd Telestash\app

# Install dependencies & run in dev mode
npm install
npm run tauri dev
```

### Build Production NSIS Installer
```powershell
npm run tauri build
```

---

## 📄 Project Notice

TeleStash is an independent project and is not affiliated with, endorsed by, or sponsored by Telegram FZ-LLC or Plex Inc. This repository does not currently publish a license file; do not assume MIT or other reuse rights unless they are explicitly added.

> **Disclaimer**: Use TeleStash responsibly and in compliance with Telegram's Terms of Service and applicable law.
