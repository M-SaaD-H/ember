# Ember

Ember is a Redis-inspired in-memory key-value store built in Rust to explore systems programming, concurrency, networking, and high-performance backend design.

The project focuses on understanding how modern in-memory systems handle concurrent workloads, protocol parsing, persistence, expiration, and efficient request processing.

---

## Features

- Concurrent request handling
- Asynchronous runtime using Tokio
- TCP server implementation
- Thread-safe shared in-memory datastore
- Redis Serialization Protocol (RESP) support
- Key expiration support
- Eviction policies
- Persistence support
- Transaction support using commands like `MULTI`, `EXEC`, and `DISCARD`
- Redis-compatible benchmarking using `redis-benchmark`
- Lightweight and modular architecture

---

## Supported Commands

| Command                                | Description                              |
|----------------------------------------|------------------------------------------|
| `PING`                                 | Health check                             |
| `ECHO message`                         | Echo back the provided message           |
| `SET key value [EX seconds\|PX ms]`    | Store a value (optionally with expiry)   |
| `GET key`                              | Retrieve a value                         |
| `DELETE key`                           | Delete a key                             |
| `LPUSH key value [value ...]`          | Push one or more values to list head     |
| `RPUSH key value [value ...]`          | Push one or more values to list tail     |
| `LRANGE key start stop`                | Read a range of list elements            |
| `EXPIRE key duration [NX\|XX\|GT\|LT]` | Set expiration with optional condition   |
| `EXISTS key`                           | Check key existence                      |
| `TTL key`                              | Get remaining TTL                        |
| `MULTI`                                | Start a transaction block                |
| `EXEC`                                 | Execute all commands in a block          |
| `DISCARD`                              | Discard transaction commands             |
| `SAVE`                                 | Persist in-memory state to disk          |

---

## Transactions

Ember supports transactional execution, enabling clients to group multiple commands in a transaction block. Using `MULTI`, clients begin a transaction and queue commands for atomic execution, which can then be finalized with `EXEC` or canceled with `DISCARD`. Transactions help ensure predictable batch operation semantics, similar to Redis. This functionality was implemented to explore transactional concepts and command queuing within the server.

Programmatic workflow:
```
MULTI
OK

SET key1 value1
QUEUED

SET key2 value2
QUEUED

EXEC
1) OK
2) OK
```
If you run `DISCARD` instead of `EXEC`, all queued commands are abandoned.

---

## Architecture

Ember follows an asynchronous event-driven architecture.

```text
             Client
               ↓
           TCP Listener
               ↓
      Async Connection Handler
               ↓
           RESP Parser
               ↓
     Command Execution Layer
               ↓
     Command Dispatcher Layer
               ↓
     Shared In-Memory Store
```

## Concurrency Model

Ember uses asynchronous request handling powered by Tokio to support multiple concurrent client connections efficiently.

Key areas explored during development:

- Concurrent socket handling
- Shared state synchronization
- Request parsing performance
- Expiration handling
- Async task scheduling


## Persistence

Ember includes persistence support to retain data across server restarts.

The persistence layer was implemented to explore:

- Disk-backed state storage
- Serialization strategies
- Recovery workflows
- Tradeoffs between durability and performance


## Expiration and Eviction

The datastore supports key expiration with TTL-based invalidation.

Eviction mechanisms were added to explore:

- Memory management strategies
- Automatic cleanup
- Expired key handling under concurrent workloads


## Running the Project

### Clone the Repository
```
git clone https://github.com/M-SaaD-H/ember.git
cd ember
```

### Build
```
cargo build --release
```

### Run
```
cargo run --release
```

The server runs on:
```
127.0.0.1:6379
```

## Project Goals

This project was built to gain hands-on experience with:

- Systems programming in Rust
- Concurrent backend system design
- Async runtimes and networking
- Database internals
- Performance-oriented engineering
- Persistence and memory management


## Future Improvements

Planned areas of exploration include:

- Pub/Sub support
- Replication
- Clustered architecture
- Advanced eviction strategies
- Improved profiling and observability
- Optimized persistence mechanisms
- Enhanced transactional guarantees

## License

This project is licensed under the [MIT License](LICENSE).