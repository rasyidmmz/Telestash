# TeleStash — Master AI Agent Handoff Guide

> **Important**: This document provides the complete, authoritative operational context for AI agents (Claude Code, Codex, Cursor, Antigravity, Windsurf, Copilot) taking over development, maintenance, or feature expansion in the TeleStash repository.

---

## 1. Quick Orientation & Context Hierarchy

| Layer | Path | Purpose |
| :--- | :--- | :--- |
| **Agent Rules** | [`AGENTS.md`](AGENTS.md) | Immutable engineering constraints, release boundaries, and testing commands. |
| **Product Spec** | [`PRD.md`](PRD.md) | Complete Product Requirements Document & feature architecture. |
| **Architecture Records** | [`docs/decisions/`](docs/decisions/) | ADR-001 through ADR-005 capturing critical architectural choices. |
| **Release Runbook** | [`docs/RELEASE_RUNBOOK.md`](docs/RELEASE_RUNBOOK.md) | Tagging, signing, and GitHub Actions CI/CD release procedures. |
| **Changelog** | [`CHANGELOG.md`](CHANGELOG.md) | Chronological version history from v1.0.0 to v1.2.2. |

---

## 2. Platform & Toolchain Invariants

1. **Target Platform**: Windows 11 64-bit (`x86_64-pc-windows-msvc`) **ONLY**. Do NOT add cross-platform abstractions, Linux/macOS targets, or mobile configurations.
2. **Connection Path**: **Direct Telegram MTProto ONLY**. TeleStash does NOT use intermediate proxies, VPNs, or network hooks.
3. **Frontend Stack**: `app/` — React 18, TypeScript 5, Vite, Tailwind CSS v4, TanStack Query v5.
4. **Backend Stack**: `app/src-tauri/` — Tauri v2, Rust 2021 (Tokio, Grammers MTProto, SQLite).
5. **Media Engines**: Bundled MPV sidecar and Whisper CLI located in `app/src-tauri/resources/`.

---

## 3. Critical Architectural Subsystems

### A. Large-File Split Transfer (>2GB) & Checkpoints
- Telegram's single-file upload limit via standard bot/user API is `2_000_000_000` bytes.
- TeleStash automatically divides files $>2$ GB into 512 MiB chunks with the caption prefix `[telestash-part]`.
- A cryptographic JSON manifest (`.tdmanifest.json`) links all split parts. In the UI, the split parts are concealed, and only the unified file is displayed.
- Checkpoints are saved in SQLite (`upload_checkpoints` and `download_checkpoints`).

### B. MPV Sidecar Streaming & Subtitle Ingestion
- Media is streamed directly from MTProto to MPV via a local loopback HTTP server with a strict 16 MiB ring buffer.
- Subtitle sidecars uploaded via TeleStash are tagged with `#telestash_sub:` in Telegram messages and concealed from normal file listing.
- When playing, TeleStash caches sidecars locally and passes `--sub-file` and `--slang=en,eng,id,ind` to MPV.

### C. Granular Transfer Pause & Resume
- Individual and batch Pause/Resume controls in the transfer queue.
- Pausing suspends I/O via async wakers (`Poll::Pending`), consuming 0% CPU and 0% network bandwidth.
- States are persisted in Tauri Store (`transfers.json`) to survive app restarts.

### D. Resizable Sidebar & Search Filtering
- Left navigation sidebar width is adjustable from `200px` to `520px` with a drag handle and stored in `localStorage` (`telestash_sidebar_width`).
- Global search explicitly filters out split parts, manifests, and subtitle messages in both Rust (`extract_search_files`) and React (`DesktopDashboard.tsx`).

---

## 4. Key Verification & Development Commands

Always run these commands from `app/`:

```bash
# 1. Run all 4 automated unit test suites
npm test

# 2. Type-check and build frontend
npm run build

# 3. Version synchronization check
node scripts/verify-versions.js

# 4. Updater signing key validation
node scripts/verify-updater-signing-key.js
```

---

## 5. Release & Git Policy (Strict Rules)

1. **Explicit Approval Required**: Never commit, push, create tags, or trigger CI workflows without explicit user confirmation in the conversation.
2. **5-Way Version Synchronization**: Version bumps must be identical in:
   - `app/package.json`
   - `app/package-lock.json`
   - `app/src-tauri/Cargo.toml`
   - `app/src-tauri/Cargo.lock`
   - `app/src-tauri/tauri.conf.json`
   - Matching `## [x.y.z]` entry in `CHANGELOG.md`
   - Use: `node app/scripts/bump-version.js <new-version>`
3. **Mandatory README Updates**: Material feature additions must be documented in `README.md`.
