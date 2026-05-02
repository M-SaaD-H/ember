//! Integration tests for the RESP protocol parser.
//!
//! Tests are grouped by type (Simple String, Integer, Bulk String, …)
//!
//! All assertions go through helpers in `common` that carry `#[track_caller]`
//! so a failure always points to the exact `assert_parse_ok` call site - not
//! inside the helper.

mod common;

use ember::resp::parser::ParseError;
use ember::resp::parser::Parser;
use ember::resp::types::RespType;

use common::{assert_consumed, assert_parse_ok, buf};

// Incomplete / empty input

#[test]
fn parse_returns_incomplete_for_empty_buffer() {
    let result = Parser::parse(&buf(""));
    assert!(
        matches!(result, Err(ParseError::Incomplete)),
        "empty buffer must be Incomplete, got {result:?}"
    );
}

#[test]
fn parse_returns_incomplete_when_crlf_missing() {
    // Each case has a valid type byte but no terminating \r\n
    for input in ["+OK", ":-42", "_"] {
        let result = Parser::parse(&buf(input));
        assert!(
            matches!(result, Err(ParseError::Incomplete)),
            "{input:?}: expected Incomplete, got {result:?}"
        );
    }
}

#[test]
fn parse_bulk_string_incomplete_body() {
    // Length header is present but body is truncated
    let result = Parser::parse(&buf("$5\r\nhel"));
    assert!(matches!(result, Err(ParseError::Incomplete)));
}

// Simple String

#[test]
fn parse_simple_string_variants() {
    assert_parse_ok("+OK\r\n", RespType::SimpleString("OK".into()));
    assert_parse_ok("+\r\n", RespType::SimpleString("".into()));
    assert_parse_ok("+hello world\r\n", RespType::SimpleString("hello world".into()));
}

#[test]
fn parse_simple_string_byte_count() {
    // '+' + "OK" + "\r\n" = 5 bytes
    assert_consumed("+OK\r\n", 5);
}

// Simple Error

#[test]
fn parse_simple_error() {
    // The parser currently returns a SimpleString for '-' — same branch
    assert_parse_ok(
        "-ERR unknown command\r\n",
        RespType::SimpleString("ERR unknown command".into()),
    );
}

// Integer

#[test]
fn parse_integer_variants() {
    assert_parse_ok(":0\r\n", RespType::Integer(0));
    assert_parse_ok(":42\r\n", RespType::Integer(42));
    assert_parse_ok(":-1\r\n", RespType::Integer(-1));
    assert_parse_ok(":1000000\r\n", RespType::Integer(1_000_000));
}

#[test]
fn parse_integer_byte_count() {
    // ':' + "1234" + "\r\n" = 7 bytes
    assert_consumed(":1234\r\n", 7);
}

// Bulk String

#[test]
fn parse_bulk_string_variants() {
    assert_parse_ok("$5\r\nhello\r\n", RespType::BulkString("hello".into()));
    assert_parse_ok("$0\r\n\r\n", RespType::BulkString("".into()));
}

#[test]
fn parse_bulk_string_byte_count() {
    // "$5\r\n" (4) + "hello" (5) + "\r\n" (2) = 11
    assert_consumed("$5\r\nhello\r\n", 11);
}

// Array

#[test]
fn parse_array_empty() {
    assert_parse_ok("*0\r\n", RespType::Array(vec![]));
}

#[test]
fn parse_array_of_bulk_strings() {
    let input = "*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n";
    assert_parse_ok(
        input,
        RespType::Array(vec![
            RespType::BulkString("foo".into()),
            RespType::BulkString("bar".into()),
        ]),
    );
    assert_consumed(input, input.len());
}

#[test]
fn parse_array_mixed_types() {
    let input = "*2\r\n:42\r\n+OK\r\n";
    assert_parse_ok(
        input,
        RespType::Array(vec![
            RespType::Integer(42),
            RespType::SimpleString("OK".into()),
        ]),
    );
}

// Null

#[test]
fn parse_null() {
    assert_parse_ok("_\r\n", RespType::Null);
}

// Boolean

#[test]
fn parse_boolean_variants() {
    assert_parse_ok("#t\r\n", RespType::Boolean(true));
    assert_parse_ok("#f\r\n", RespType::Boolean(false));
}

#[test]
fn parse_boolean_invalid_value_is_error() {
    let result = Parser::parse(&buf("#x\r\n"));
    assert!(
        matches!(result, Err(ParseError::Invalid(_))),
        "expected Invalid, got {result:?}"
    );
}

// Inline commands (legacy)

#[test]
fn parse_inline_single_word() {
    assert_parse_ok(
        "PING\r\n",
        RespType::Array(vec![RespType::BulkString("PING".into())]),
    );
}

#[test]
fn parse_inline_multi_word() {
    let input = "SET foo bar\r\n";
    assert_parse_ok(
        input,
        RespType::Array(vec![
            RespType::BulkString("SET".into()),
            RespType::BulkString("foo".into()),
            RespType::BulkString("bar".into()),
        ]),
    );
    assert_consumed(input, input.len());
}
