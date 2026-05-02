//! Integration tests for Redis command parsing (`extract_command`).
//!
//! All tests work through the public API: they build a RESP Array of
//! BulkStrings (the canonical wire format) and call `extract_command`,
//! verifying the resulting `Command` variant and its fields.

mod common;

use ember::command::command::{extract_command, Command};
use ember::resp::types::RespType;

use common::make_cmd;

// PING

#[test]
fn extract_ping() {
    assert!(matches!(
        extract_command(&make_cmd(&["PING"])).unwrap(),
        Command::PING
    ));
}

#[test]
fn extract_ping_case_insensitive() {
    assert!(matches!(extract_command(&make_cmd(&["ping"])).unwrap(), Command::PING));
    assert!(matches!(extract_command(&make_cmd(&["Ping"])).unwrap(), Command::PING));
}

// ECHO

#[test]
fn extract_echo() {
    let cmd = extract_command(&make_cmd(&["ECHO", "hello"])).unwrap();
    assert!(matches!(cmd, Command::ECHO(ref s) if s == "hello"));
}

// SET

#[test]
fn extract_set_without_expiry() {
    let cmd = extract_command(&make_cmd(&["SET", "mykey", "myval"])).unwrap();
    assert!(matches!(cmd, Command::SET(ref k, ref v, None) if k == "mykey" && v == "myval"));
}

#[test]
fn extract_set_with_ex_flag() {
    let cmd = extract_command(&make_cmd(&["SET", "k", "v", "EX", "10"])).unwrap();
    assert!(matches!(cmd, Command::SET(_, _, Some(_))));
}

#[test]
fn extract_set_with_px_flag() {
    let cmd = extract_command(&make_cmd(&["SET", "k", "v", "PX", "5000"])).unwrap();
    assert!(matches!(cmd, Command::SET(_, _, Some(_))));
}

#[test]
fn extract_set_missing_value_returns_error() {
    assert!(extract_command(&make_cmd(&["SET", "only_key"])).is_err());
}

// GET

#[test]
fn extract_get() {
    let cmd = extract_command(&make_cmd(&["GET", "mykey"])).unwrap();
    assert!(matches!(cmd, Command::GET(ref k) if k == "mykey"));
}

// DELETE

#[test]
fn extract_delete() {
    let cmd = extract_command(&make_cmd(&["DELETE", "mykey"])).unwrap();
    assert!(matches!(cmd, Command::DELETE(ref k) if k == "mykey"));
}

// LPUSH / RPUSH

#[test]
fn extract_lpush_preserves_value_order() {
    let cmd = extract_command(&make_cmd(&["LPUSH", "list", "a", "b", "c"])).unwrap();
    if let Command::LPUSH(key, vals) = cmd {
        assert_eq!(key, "list");
        assert_eq!(vals, vec!["a", "b", "c"]);
    } else {
        panic!("expected Command::LPUSH");
    }
}

#[test]
fn extract_rpush_preserves_value_order() {
    let cmd = extract_command(&make_cmd(&["RPUSH", "list", "x", "y"])).unwrap();
    if let Command::RPUSH(key, vals) = cmd {
        assert_eq!(key, "list");
        assert_eq!(vals, vec!["x", "y"]);
    } else {
        panic!("expected Command::RPUSH");
    }
}

// LRANGE

#[test]
fn extract_lrange_positive_indices() {
    let cmd = extract_command(&make_cmd(&["LRANGE", "list", "0", "4"])).unwrap();
    assert!(matches!(cmd, Command::LRANGE(ref k, 0, 4) if k == "list"));
}

#[test]
fn extract_lrange_negative_stop() {
    let cmd = extract_command(&make_cmd(&["LRANGE", "list", "0", "-1"])).unwrap();
    assert!(matches!(cmd, Command::LRANGE(ref k, 0, -1) if k == "list"));
}

// EXPIRE

#[test]
fn extract_expire_without_option() {
    let cmd = extract_command(&make_cmd(&["EXPIRE", "key", "30"])).unwrap();
    assert!(matches!(cmd, Command::EXPIRE(ref k, _, None) if k == "key"));
}

#[test]
fn extract_expire_with_nx_option() {
    let cmd = extract_command(&make_cmd(&["EXPIRE", "key", "30", "NX"])).unwrap();
    assert!(matches!(cmd, Command::EXPIRE(_, _, Some(ref opt)) if opt == "NX"));
}

// MULTI / EXEC / DISCARD

#[test]
fn extract_transaction_commands() {
    assert!(matches!(
        extract_command(&make_cmd(&["MULTI"])).unwrap(),
        Command::MULTI
    ));
    assert!(matches!(
        extract_command(&make_cmd(&["EXEC"])).unwrap(),
        Command::EXEC
    ));
    assert!(matches!(
        extract_command(&make_cmd(&["DISCARD"])).unwrap(),
        Command::DISCARD
    ));
}

// SAVE

#[test]
fn extract_save() {
    assert!(matches!(
        extract_command(&make_cmd(&["SAVE"])).unwrap(),
        Command::SAVE
    ));
}

// Error paths

#[test]
fn extract_unknown_command_returns_error() {
    assert!(extract_command(&make_cmd(&["FOOBAR"])).is_err());
}

#[test]
fn extract_empty_array_returns_error() {
    assert!(extract_command(&RespType::Array(vec![])).is_err());
}

#[test]
fn extract_non_array_returns_error() {
    assert!(extract_command(&RespType::SimpleString("PING".into())).is_err());
    assert!(extract_command(&RespType::BulkString("PING".into())).is_err());
}
