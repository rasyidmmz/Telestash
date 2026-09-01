# ADR-003: Large-File Split Engine (>2GB) and Resumable Checkpoints

## Status: Accepted
## Date: 2026-08-01

## Context
Telegram enforces a 2,000,000,000 byte limit for standard media uploads. High-bitrate movies frequently exceed this size.

## Decision
Automatically divide files $>2$ GB into 512 MiB part messages tagged with `[telestash-part]` and generate a cryptographic `.tdmanifest.json` manifest. Store transfer progress in SQLite `upload_checkpoints` and `download_checkpoints`.

## Consequences
- Enables uploading files of arbitrary size (4K Blu-ray remuxes, large season packs).
- Conceals split chunks from user view, presenting unified media files in the UI.
- Interrupted transfers resume seamlessly from the last completed part.
