//! Shared test utilities for Ember's integration test suite.
//!
//! Cargo does not treat subdirectories inside `tests/` as standalone test
//! targets, so this module is compiled only when a test file explicitly
//! imports it with `mod common;`.
//!
//! Patterns applied here:
//! - `#[track_caller]` on assertion helpers so that failures report the
//!   test line, not the helper's line.
//! - `#[allow(dead_code)]` because not every helper is used by every file.
//! - A RAII `TestDb` fixture (from mini-redis) for panic-safe setup/teardown.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use bytes::BytesMut;
use ember::database::core::{RedisObject, DB};
use ember::resp::parser::Parser;
use ember::resp::types::RespType;

// Byte-buffer helper

// Wrap a `&str` in a `BytesMut` ready for the RESP parser.
pub fn buf(s: &str) -> BytesMut {
    BytesMut::from(s.as_bytes())
}

// RESP assertion helpers

// Assert that parsing `input` succeeds and yields `expected`.

// `#[track_caller]` ensures a failing assertion points to the test line,
#[track_caller]
pub fn assert_parse_ok(input: &str, expected: RespType) {
    let (got, _) = Parser::parse(&buf(input))
        .unwrap_or_else(|e| panic!("parse({input:?}) failed: {e}"));
    assert_eq!(got, expected);
}

// Assert the number of bytes the parser consumed for `input`.
#[track_caller]
pub fn assert_consumed(input: &str, expected_bytes: usize) {
    let (_, consumed) = Parser::parse(&buf(input))
        .unwrap_or_else(|e| panic!("parse({input:?}) failed: {e}"));
    assert_eq!(
        consumed, expected_bytes,
        "wrong byte count for {input:?}: got {consumed}, expected {expected_bytes}"
    );
}

// Command builder helper

// Build a RESP Array of BulkStrings — the canonical on-wire format for
// Redis commands sent from a client.
pub fn make_cmd(parts: &[&str]) -> RespType {
    RespType::Array(
        parts
            .iter()
            .map(|s| RespType::BulkString(s.to_string()))
            .collect(),
    )
}

// DB test fixture

// RAII wrapper around an in-memory [`DB`].

// Inspired by mini-redis's `DbDropGuard`: setup happens in [`TestDb::new`],
// and any future cleanup would live in a `Drop` impl ensuring teardown even
// if the test panics.
pub struct TestDb {
    pub db: DB,
}

impl TestDb {
    // Create a fresh, empty in-memory database with no background tasks.
    pub fn new() -> Self {
        TestDb {
            db: DB::new_in_memory(),
        }
    }

    // Set a string KV pair with no expiry.
    pub fn set(&self, key: &str, value: &str) {
        self.db
            .set(
                key.to_string(),
                RedisObject::String(value.to_string()),
                None,
            )
            .unwrap_or_else(|e| panic!("set({key:?}, {value:?}) failed: {e}"));
    }

    // Set a string key that is already expired (expires 100 ms in the past).
    pub fn set_expired(&self, key: &str, value: &str) {
        let past = Instant::now()
            .checked_sub(Duration::from_millis(100))
            .expect("system clock too new");
        self.db
            .set(
                key.to_string(),
                RedisObject::String(value.to_string()),
                Some(past),
            )
            .unwrap_or_else(|e| panic!("set_expired({key:?}) failed: {e}"));
    }

    // Get the string value for `key`. Returns `"nil"` for missing/expired keys.
    pub fn get_string(&self, key: &str) -> String {
        match self.db.get(key.to_string()).unwrap_or_else(|e| panic!("get({key:?}) failed: {e}")) {
            RedisObject::String(s) => s,
            other => panic!("expected RedisObject::String for {key:?}, got {other:?}"),
        }
    }
}
