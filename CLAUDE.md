# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Freight** is a high-performance asset/data server written in Rust. It offloads binary game asset delivery (icons, effects, maps, NPCs, tilesets, etc.) from a Java game logic server to reduce its bandwidth by ~70-85%. Unity clients connect via TCP and request assets using a custom binary protocol with XOR encryption.

## Build & Run

```bash
cd server
cargo build --release   # optimized binary → target/release/freight
cargo build             # debug build
cargo run               # run with debug build + config.yml in working dir
```

The server reads `./config.yml` at startup. If not found, it falls back to hardcoded defaults (port 14450, host 0.0.0.0).

## Architecture

### Communication Flow
```
Unity Client → TCP → Freight Server (Rust)   [asset delivery]
Unity Client → TCP → Game Logic Server (Java) [game logic]
```

### Module Responsibilities

| Module | Purpose |
|--------|---------|
| `main.rs` | TCP listener, connection spawner, periodic stats logging, graceful shutdown |
| `config.rs` | YAML config loading + path resolution via `{base}`, `{zoom}`, `{id}`, `{name}` placeholders |
| `protocol.rs` | XOR stream cipher (LCG seeded), binary packet encoding/decoding, command constants |
| `session.rs` | Per-connection lifecycle: key exchange → init → message loop; rate limiter (200 req/10s) |
| `handler.rs` | Routes command bytes to data loading logic |
| `data.rs` | DashMap-based LRU file cache; `img_by_name` index scanned at startup |
| `metrics.rs` | Atomic counters for bandwidth, connections, requests, cache hits |
| `session_mgr.rs` | Concurrent session registry (DashMap); tracks per-session metadata |

### Connection Protocol Sequence
1. Client connects → server generates 32-byte XOR key (`GET_SESSION_ID = -27`)
2. Client sends `FREIGHT_INIT (1)` with zoom level & screen dimensions
3. Client sends data request commands; server responds with encrypted binary data
4. Sessions are disconnected after 300s idle

### Binary Packet Format
- **Normal packets**: `[cmd: i8][len_hi: i8][len_lo: i8][data: encrypted bytes]`
- **Big data packets** (commands: -32, -66, 11, -67, -74, -87, 66, 12): 3-byte length encoding
  - `len = ((b0 + 128) | ((b1 + 128) << 8) | ((b2 + 128) << 16))`
- `GET_SESSION_ID` is the only unencrypted exchange; all others use the XOR stream cipher

### Asset Path Resolution

Paths are configured in `config.yml` and resolved via `config.rs`. Key path patterns:
- Icons: `{base}/x{zoom}/icon/{id}.png`
- Effects: `{base}/x{zoom}/effect/data/DataEffect_{id}` + `img/ImgEffect_{id}.png`
- Maps: `{base}/x{zoom}/map/{id}.dat`
- NPCs/mobs: `{base}/x{zoom}/mob/{id}`
- Binaries (shared): `{base}/binary/head_avatar.bin`, `item_template.bin`, etc.
- Named images: `{base}/x{zoom}/img_by_name/{name}.png`

Zoom levels supported: `x1`, `x2`, `x4`. Default zoom is `2` (configurable).

### Caching Strategy
- `DataStore` uses a `DashMap` for lock-free concurrent access
- LRU eviction runs when cache exceeds `max_cache_mb` (default 512 MB)
- `img_by_name` index is built at startup by scanning `img_by_name/` directories for each zoom level
- Cache keys are file paths (strings); entries store raw bytes + last-access timestamp

### NPC Special Cases
NPC IDs 82, 88, and 89 have special path handling in `handler.rs` — check that file when modifying NPC loading logic.

## Key Configuration (`server/config.yml`)

```yaml
server:
  host: "0.0.0.0"
  port: 14450
  max_connections: 10000
  default_zoom: 2
  idle_timeout_secs: 300
  max_cache_mb: 512
  stats_interval_secs: 60
```

All asset paths are defined under the `paths:` key and resolved by `config.rs`.

## Dependencies

- **tokio** — async runtime (full features)
- **dashmap** — lock-free concurrent HashMap for cache and session registry
- **serde / serde_yaml** — config deserialization
- **bytes** — byte buffer manipulation
- **tracing / tracing-subscriber** — structured logging
- **thiserror** — error type derivation
