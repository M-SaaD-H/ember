# Ember


Ember is a High-Performance, Concurrent, Redis-compatible Key-Value Store in Rust engineered for high throughput and low-latency workloads. Built from the ground up in Rust using the Tokio asynchronous runtime, Ember implements a sharded concurrency model and optimized network I/O to handle hundreds of thousands of requests per second on commodity hardware.

The project serves as a deep dive into systems programming, specifically focusing on protocol design (RESP), lock-free data structures, asynchronous networking, and database persistence internals.

## Core Features

- **Redis Compatibility**: Implements the Redis Serialization Protocol (RESP) and supports standard commands like `SET`, `GET`, `LPUSH`, `LRANGE`, and `EXPIRE`.
- **High Concurrency**: Utilizes a sharded hash map (`DashMap`) to eliminate global lock contention.
- **Advanced I/O**: Multi-acceptor model using `SO_REUSEPORT` for kernel-level load balancing across CPU cores.
- **Zero-Copy Parsing**: Efficient RESP parsing using `bytes::BytesMut` to minimize heap allocations in the hot path.
- **Persistence**: Point-in-time RDB snapshotting with atomic file swaps.
- **Expiration Engine**: Hybrid lazy and active key expiration using a probabilistic sampling algorithm.
- **Pipelining Support**: Batch-aware processing with `BufWriter` for coalesced network syscalls.

## Architecture

Ember's architecture is designed to scale linearly with available CPU cores by removing serial bottlenecks at both the networking and storage layers.

### Parallel Acceptor Model
Instead of a single accept loop, a common bottleneck in high-RPS servers. Ember spawns one `TcpListener` per logical CPU core. By leveraging the `SO_REUSEPORT` socket option, the Linux kernel distributes incoming SYN packets across these listeners using a flow-based hash. This eliminates userspace contention during connection establishment and allows the server to scale to millions of concurrent connections.

### Sharded Concurrency
Ember avoids the "Global Lock" problem typical of simple in-memory stores. The primary database state is managed via `DashMap`, which shards the keyspace across 64 independent `RwLock` segments. This ensures that operations on different keys can proceed in parallel without blocking, providing high-performance concurrent access even under heavy write pressure.

### I/O Optimization & Pipelining
The network stack is tuned for throughput:
- **`BufWriter` Coalescing**: Small RESP responses (e.g., `+OK\r\n`) are buffered in userspace (64 KiB) and flushed in batches, reducing `write()` syscall overhead by up to 8x.
- **`TCP_NODELAY`**: Nagle's algorithm is disabled to ensure immediate delivery of responses, critical for low-latency command/response cycles.
- **Pipeline Draining**: The server drains all complete commands from the read buffer before issuing a new `read()` syscall, maximizing the throughput of pipelined clients.

## Persistence

Ember implements an RDB-inspired snapshotting mechanism for durability.

- **Point-in-Time Snapshots**: When a snapshot is triggered, Ember takes a consistent view of the `DashMap` shards and serializes them to an RDB file.
- **Non-Blocking Writes**: Serialization happens in a background task, ensuring the main I/O event loop is never blocked by disk I/O.
- **Atomic Renames**: Snapshots are first written to a temporary file (`.tmp`) and then moved to the final destination using `std::fs::rename`, ensuring that a crash during persistence never leaves the database in a corrupted state.

## Expiration & Eviction

Ember uses a two-pronged strategy for managing key lifetimes, modeled after Redis's own expiration engine:

1.  **Lazy Expiration**: Every `GET` or `LRANGE` request first checks the expiration metadata. If the key has passed its TTL, it is deleted immediately, and `nil` is returned.
2.  **Active Expiration**: A background reaper task runs every 100ms. It samples 20 random keys from the expiration map:
    - Any expired keys are removed.
    - If more than 25% of the sampled keys were expired, the task repeats immediately to aggressively clear memory.
    - This probabilistic approach ensures that expired keys are eventually cleared without requiring an $O(n)$ scan of the entire database.

## Benchmarking

Ember is fully compatible with `redis-benchmark`. To evaluate performance, ensure the server is compiled in `--release` mode.

### Sample Benchmark
```bash
# Test GET performance with 50 concurrent clients and 100k requests
redis-benchmark -p 6379 -t set,get -n 100000 -q

# Test pipelining performance (16 commands per batch)
redis-benchmark -p 6379 -P 16 -c 100 -t set -n 1000000 -q
```

*Note: Performance varies based on kernel tuning (e.g., `somaxconn`, `ulimit`) and hardware, but Ember is designed to saturated 10GbE links on modern Linux systems.*

## Engineering Learnings & Tradeoffs

- **Memory vs. Latency**: Using `Arc` and `DashMap` provides high concurrency but introduces slight memory overhead compared to a single-threaded event loop (like standard Redis). However, for multi-core systems, the throughput gains significantly outweigh the overhead.
- **Async Overhead**: While `tokio` introduces a small runtime cost, it allows Ember to handle massive connection counts that would be impossible with a thread-per-connection model.
- **Zero-Copy Challenges**: Implementing zero-copy RESP parsing required careful management of `Bytes` reference counting to ensure memory is reclaimed promptly while avoiding unnecessary clones of large string values.

## Usage

### Building from source
```bash
cargo build --release
./target/release/ember
```

### Basic Commands
```bash
# Set a key with 60 second expiration
redis-cli SET session_id "abc-123" EX 60

# Push to a list
redis-cli LPUSH tasks "email_user" "resize_image"

# Range query
redis-cli LRANGE tasks 0 -1
```

## Future Roadmap

- **AOF (Append Only File)**: Implement write-ahead logging for higher durability guarantees.
- **LRU Eviction**: Implement a Least Recently Used (LRU) policy to handle memory pressure when the max-memory limit is reached.
- **Cluster Support**: Hash-based sharding across multiple Ember nodes.
- **Extended Types**: Support for Sets, Sorted Sets (Skip Lists), and Hashes.

## License

This project is licensed under the [MIT License](LICENSE).