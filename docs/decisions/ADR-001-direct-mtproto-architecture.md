# ADR-001: Direct Telegram MTProto Architecture via Grammers

## Status: Accepted
## Date: 2026-07-26

## Context
TeleStash requires high-throughput media ingestion and streaming directly to/from Telegram servers without intermediate proxy servers, VPNs, or third-party cloud hosting costs.

## Decision
Use direct MTProto connection via the Rust `grammers` asynchronous client library with SQLite metadata caching.

## Consequences
- Zero intermediate infrastructure costs.
- Direct cryptographic security between client and Telegram Data Centers.
- Application must manage MTProto flood waits and chunking natively.
