# ADR-004: External Subtitles Sidecar Linkage with Zero-Clutter Storage

## Status: Accepted
## Date: 2026-08-24

## Context
Users frequently acquire external subtitle tracks (`.srt`, `.ass`, `.sub`/`.idx` VobSub) that need to be linked to remote media files without cluttering folder file views.

## Decision
Upload external subtitle sidecars with `#telestash_sub:` caption tagging. Conceal them from standard folder listings, automatically pair them using `subtitleMatcher.ts`, cache them under dual keys (message ID and video filename stem), and pass them to MPV via `--sub-file`.

## Consequences
- 100% clean series folder view.
- Supports both text subtitles (`.srt`, `.ass`) and bitmap VobSub pairs (`.sub` + `.idx`).
- Seamless multi-episode playlist auto-loading.
