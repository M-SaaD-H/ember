# Ember

A small **Redis-compatible** in-memory server written in **Rust**. I built it to learn how Redis works under the hood and to get more comfortable with async Rust (Tokio), the RESP wire protocol, and structuring a network service.

More Redis features will be added over time.

## What works today

- **TCP server** on `127.0.0.1` (default port **6379**, overridable with `--port`).
- **Concurrent clients** via Tokio tasks.
- **In-memory store** backed by `Arc<Mutex<...>>` for shared mutable state.
- **RESP support** for simple strings, bulk strings, arrays, integers, null, and booleans.
- **Redis-like commands** (implemented subset):
  - Core: `PING`, `ECHO`, `SET`, `GET`, `DELETE`
  - Expiration: `SET ... EX|PX`, `EXPIRE key milliseconds [NX|XX|GT|LT]`
  - Lists: `LPUSH`, `RPUSH`, `LRANGE`
  - Transactions: `MULTI`, `EXEC`, `DISCARD`
  - Persistence: `SAVE`
- **Expiration handling** with both lazy expiration checks and a periodic active expiration cycle.
- **RDB snapshot persistence**:
  - Loads snapshot from `snapshots/client-0001.rdb` on startup (if present)
  - Saves snapshot atomically when `SAVE` is called

You can talk to it with **`redis-cli`** like a real Redis instance for these commands.

## Requirements

- A recent **Rust** toolchain (`rustup` recommended) that supports the edition declared in `Cargo.toml`.

## Build and run

```bash
cargo build --release
cargo run --release
```

Custom port:

```bash
cargo run --release -- --port 1234
```

Optional: enable logs (uses `env_logger`), e.g.:

```bash
RUST_LOG=info cargo run --release
```

## Try it

With another terminal and `redis-cli` installed:

```bash
redis-cli -p 6379 PING
redis-cli -p 6379 ECHO hello
redis-cli -p 6379 SET mykey "hello world"
redis-cli -p 6379 GET mykey
redis-cli -p 6379 SET temp value EX 10
redis-cli -p 6379 EXPIRE temp 5000 NX
redis-cli -p 6379 DELETE mykey

redis-cli -p 6379 LPUSH mylist a b c
redis-cli -p 6379 RPUSH mylist d e
redis-cli -p 6379 LRANGE mylist 0 -1

redis-cli -p 6379 <<'EOF'
MULTI
SET txkey one
GET txkey
EXEC

redis-cli -p 6379 SAVE
```

## Stack

- **Tokio** — async runtime, TCP, I/O
- **bytes** — buffering
- **anyhow** — error handling
- **log** / **env_logger** — logging

## Roadmap

The goal is to keep expanding Redis compatibility: richer data types (hashes, sets, sorted sets, streams), replication, pub/sub, stricter protocol and reply compatibility, and more complete persistence behavior.

## License

This project is licensed under the [MIT License](LICENSE).

---

*Educational project; not a production Redis replacement.*
