# ADR-002: Native MPV Sidecar for Zero-Copy Cinema Playback

## Status: Accepted
## Date: 2026-07-30

## Context
Standard web browsers (and Webview2) cannot natively decode 10-bit HEVC/H.265, 4K HDR, AV1, or multi-channel surround audio (Dolby Atmos, DTS-HD) inside MKV containers without transcoding.

## Decision
Bundle native 64-bit MPV executable as a Tauri sidecar and stream media over a local HTTP loopback server with a strict 16 MiB in-memory ring buffer.

## Consequences
- 100% native hardware acceleration for all modern audio and video codecs.
- Zero CPU transcoding overhead.
- Native playlist auto-play for seamless binge-watching.
