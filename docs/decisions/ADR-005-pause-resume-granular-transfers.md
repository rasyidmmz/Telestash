# ADR-005: Async Waker-Based Granular Transfer Pause & Resume

## Status: Accepted
## Date: 2026-08-24

## Context
Pausing large transfers must immediately halt network I/O and CPU usage without severing stream state or dropping active queue items.

## Decision
Implement cooperative stream suspension using Tokio async wakers (`Poll::Pending`) and persist active transfer queue snapshots in Tauri Store.

## Consequences
- 0% CPU and 0% bandwidth consumption when paused.
- Transfers survive application restart and power cycles.
- Granular per-row pause/resume buttons and batch pause/resume controls.
