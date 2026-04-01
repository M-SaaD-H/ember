# Ember

A small **Redis-compatible** in-memory server written in **Rust**. I built it to learn how Redis works under the hood and to get more comfortable with async Rust (Tokio), the RESP wire protocol, and structuring a network service.

More Redis features will be added over time.

## What works today

- **TCP server** on `127.0.0.1` (default port **6379**, overridable with `--port`).
- **Concurrent clients** via Tokio tasks; shared state uses an in-memory `HashMap` behind `Arc<Mutex<…>>`.
- **RESP** parsing and encoding for the commands below.
- **Commands** (Redis-style names and behavior where implemented):
  - `PING` → `PONG`
  - `ECHO <message>` → bulk string reply
  - `SET key value` and `SET key value EX seconds` / `PX milliseconds` for optional expiry
  - `EXPIRE key expires_in <NX | XX | GT | LT>` to set expiry on existing keys
  - `GET key` → bulk string (missing keys are not yet identical to Redis’s null reply)

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
```

## Stack

- **Tokio** — async runtime, TCP, I/O
- **bytes** — buffering
- **anyhow** — error handling
- **log** / **env_logger** — logging

## Roadmap

The goal is to grow this toward more of Redis’s surface area: richer types (lists, hashes, sets), persistence, replication, pub/sub, and stricter protocol compatibility—implemented incrementally while keeping the codebase easy to follow for learning.

## License

This project is licensed under the [MIT License](LICENSE).

---

*Educational project; not a production Redis replacement.*
