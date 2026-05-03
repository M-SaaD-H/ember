//! Integration tests for `RespType::to_string()` serialization.
//!
//! These tests verify the exact on-wire RESP format produced for every
//! `RespType` variant, including edge cases (empty strings, nested arrays,
//! negative integers).

mod common;

use ember::resp::types::RespType;

// SimpleString

#[test]
fn simple_string_serialization() {
    assert_eq!(RespType::SimpleString("OK".into()).to_string(), "+OK\r\n");
    assert_eq!(RespType::SimpleString("".into()).to_string(), "+\r\n");
    assert_eq!(
        RespType::SimpleString("hello world".into()).to_string(),
        "+hello world\r\n"
    );
}

// SimpleError

#[test]
fn simple_error_serialization() {
    assert_eq!(
        RespType::SimpleError("ERR bad command".into()).to_string(),
        "-ERR bad command\r\n"
    );
}

// Integer

#[test]
fn integer_serialization() {
    assert_eq!(RespType::Integer(0).to_string(), ":0\r\n");
    assert_eq!(RespType::Integer(42).to_string(), ":42\r\n");
    assert_eq!(RespType::Integer(-7).to_string(), ":-7\r\n");
    assert_eq!(RespType::Integer(i64::MAX).to_string(), format!(":{}\\r\\n", i64::MAX).replace("\\r\\n", "\r\n"));
}

// BulkString

#[test]
fn bulk_string_serialization() {
    assert_eq!(
        RespType::BulkString("hello".into()).to_string(),
        "$5\r\nhello\r\n"
    );
    assert_eq!(RespType::BulkString("".into()).to_string(), "$0\r\n\r\n");
}

#[test]
fn bulk_string_length_is_byte_count_not_char_count() {
    // "café" is 4 chars but 5 UTF-8 bytes.
    // The RESP spec (https://redis.io/docs/reference/protocol-spec/) states
    // that the length prefix is the 'byte' count of the following data.
    // Using char count would produce a malformed frame: the receiver would
    // read only 4 bytes ("caf") and then interpret 'é' as the trailing CRLF.
    let s = "café";
    let serialized = RespType::BulkString(s.into()).to_string();
    assert!(
        serialized.starts_with("$5\r\n"),
        "expected byte count 5, got: {serialized:?}"
    );
}

// Array

#[test]
fn array_serialization() {
    assert_eq!(RespType::Array(vec![]).to_string(), "*0\r\n");
    assert_eq!(
        RespType::Array(vec![RespType::BulkString("foo".into())]).to_string(),
        "*1\r\n$3\r\nfoo\r\n"
    );
}

#[test]
fn nested_array_serialization() {
    let inner = RespType::Array(vec![RespType::Integer(1)]);
    let outer = RespType::Array(vec![inner]);
    assert_eq!(outer.to_string(), "*1\r\n*1\r\n:1\r\n");
}

// Null

#[test]
fn null_serialization() {
    assert_eq!(RespType::Null.to_string(), "_\r\n");
}

// Boolean

#[test]
fn boolean_serialization() {
    assert_eq!(RespType::Boolean(true).to_string(), "#t\r\n");
    assert_eq!(RespType::Boolean(false).to_string(), "#f\r\n");
}

// Round-trip: serialize then parse

#[test]
fn round_trip_simple_string() {
    use bytes::BytesMut;
    use ember::resp::parser::Parser;

    let original = RespType::SimpleString("PONG".into());
    let wire = original.to_string();
    let (parsed, _) = Parser::parse(&BytesMut::from(wire.as_bytes())).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn round_trip_bulk_string() {
    use bytes::BytesMut;
    use ember::resp::parser::Parser;

    let original = RespType::BulkString("hello world".into());
    let wire = original.to_string();
    let (parsed, _) = Parser::parse(&BytesMut::from(wire.as_bytes())).unwrap();
    assert_eq!(parsed, original);
}
