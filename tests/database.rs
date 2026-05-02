//! Integration tests for the DB core: set, get, delete, list ops, expiry.
//!
//! Uses `TestDb` from `common` - a RAII fixture wrapping `DB::new_in_memory()`.
//! Expiry is tested by passing already-past `Instant` values so no async
//! runtime or real sleeping is needed.

mod common;

use std::time::{Duration, Instant};

use ember::database::core::RedisObject;

use common::TestDb;

// Set / Get

#[test]
fn set_and_get_string() {
    let t = TestDb::new();
    t.set("name", "ember");
    assert_eq!(t.get_string("name"), "ember");
}

#[test]
fn get_missing_key_returns_nil() {
    let t = TestDb::new();
    assert_eq!(t.get_string("no_such_key"), "nil");
}

#[test]
fn set_overwrites_previous_value() {
    let t = TestDb::new();
    t.set("key", "first");
    t.set("key", "second");
    assert_eq!(t.get_string("key"), "second");
}

// Delete

#[test]
fn delete_removes_key() {
    let t = TestDb::new();
    t.set("key", "value");
    t.db.delete("key".into()).unwrap();
    assert_eq!(t.get_string("key"), "nil");
}

#[test]
fn delete_nonexistent_key_is_ok() {
    let t = TestDb::new();
    assert!(t.db.delete("ghost".into()).is_ok());
}

// Lazy expiry (get returns nil for expired keys)

#[test]
fn get_returns_nil_for_expired_key() {
    let t = TestDb::new();
    // Insert with an already-past expiry — simulates a key whose TTL elapsed
    let past = Instant::now()
        .checked_sub(Duration::from_millis(100))
        .expect("clock too new");
    t.db
        .set("key".into(), RedisObject::String("val".into()), Some(past))
        .unwrap();
    assert_eq!(t.get_string("key"), "nil");
}

#[test]
fn set_overwrites_clears_old_expiry() {
    let t = TestDb::new();
    // First write: set with an imminent expiry
    let past = Instant::now()
        .checked_sub(Duration::from_millis(100))
        .expect("clock too new");
    t.db
        .set("key".into(), RedisObject::String("old".into()), Some(past))
        .unwrap();
    // Overwrite without an expiry — key should now be persistent
    t.set("key", "new");
    assert_eq!(t.get_string("key"), "new");
}

// LPUSH

#[test]
fn lpush_creates_list_on_new_key() {
    let t = TestDb::new();
    let vals = vec![RedisObject::String("a".into())];
    t.db.lpush("list".into(), vals).unwrap();

    if let RedisObject::List(items) = t.db.lrange("list".into(), 0, -1).unwrap() {
        assert_eq!(items.len(), 1);
    } else {
        panic!("expected List");
    }
}

#[test]
fn lpush_prepends_to_existing_list() {
    let t = TestDb::new();
    t.db.lpush("list".into(), vec![RedisObject::String("b".into())]).unwrap();
    t.db.lpush("list".into(), vec![RedisObject::String("a".into())]).unwrap();

    if let RedisObject::List(items) = t.db.lrange("list".into(), 0, -1).unwrap() {
        // LPUSH prepends: "a" should be at index 0
        assert_eq!(items[0], RedisObject::String("a".into()));
        assert_eq!(items[1], RedisObject::String("b".into()));
    } else {
        panic!("expected List");
    }
}

// RPUSH

#[test]
fn rpush_appends_to_list() {
    let t = TestDb::new();
    t.db.rpush("list".into(), vec![RedisObject::String("x".into())]).unwrap();
    t.db.rpush("list".into(), vec![RedisObject::String("y".into())]).unwrap();

    if let RedisObject::List(items) = t.db.lrange("list".into(), 0, -1).unwrap() {
        assert_eq!(items[0], RedisObject::String("x".into()));
        assert_eq!(items[1], RedisObject::String("y".into()));
    } else {
        panic!("expected List");
    }
}

// LRANGE

#[test]
fn lrange_subset() {
    let t = TestDb::new();
    for v in ["a", "b", "c", "d"] {
        t.db.rpush("list".into(), vec![RedisObject::String(v.into())]).unwrap();
    }

    if let RedisObject::List(items) = t.db.lrange("list".into(), 1, 2).unwrap() {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], RedisObject::String("b".into()));
        assert_eq!(items[1], RedisObject::String("c".into()));
    } else {
        panic!("expected List");
    }
}

#[test]
fn lrange_negative_index() {
    let t = TestDb::new();
    for v in ["a", "b", "c"] {
        t.db.rpush("list".into(), vec![RedisObject::String(v.into())]).unwrap();
    }

    if let RedisObject::List(items) = t.db.lrange("list".into(), -1, -1).unwrap() {
        assert_eq!(items[0], RedisObject::String("c".into()));
    } else {
        panic!("expected List");
    }
}

// EXPIRE option flags

#[test]
fn expire_nx_sets_when_no_expiry_exists() {
    let t = TestDb::new();
    t.set("key", "val");
    let future = Instant::now() + Duration::from_secs(60);
    t.db.expire("key".into(), future, Some("NX".into())).unwrap();
    // Key should still be accessible (not expired)
    assert_eq!(t.get_string("key"), "val");
}

#[test]
fn expire_nx_does_not_overwrite_existing_expiry() {
    let t = TestDb::new();
    let first_expiry = Instant::now() + Duration::from_secs(100);
    t.db.set("key".into(), RedisObject::String("v".into()), Some(first_expiry)).unwrap();

    // NX: only set if no expiry exists — should be a no-op here
    let second_expiry = Instant::now() + Duration::from_secs(200);
    t.db.expire("key".into(), second_expiry, Some("NX".into())).unwrap();

    // Key still alive (first expiry is still in the future)
    assert_eq!(t.get_string("key"), "v");
}

#[test]
fn expire_xx_only_sets_when_expiry_exists() {
    let t = TestDb::new();
    t.set("key", "val");

    // XX: only set expiry if one already exists — key has none, so no-op
    let future = Instant::now() + Duration::from_secs(60);
    t.db.expire("key".into(), future, Some("XX".into())).unwrap();

    // Key still accessible
    assert_eq!(t.get_string("key"), "val");
}

#[test]
fn expire_gt_updates_when_new_is_greater() {
    let t = TestDb::new();
    let initial = Instant::now() + Duration::from_secs(10);
    t.db.set("key".into(), RedisObject::String("v".into()), Some(initial)).unwrap();

    let later = Instant::now() + Duration::from_secs(100);
    t.db.expire("key".into(), later, Some("GT".into())).unwrap();

    assert_eq!(t.get_string("key"), "v");
}

#[test]
fn expire_lt_updates_when_new_is_lesser() {
    let t = TestDb::new();
    let initial = Instant::now() + Duration::from_secs(100);
    t.db.set("key".into(), RedisObject::String("v".into()), Some(initial)).unwrap();

    let sooner = Instant::now() + Duration::from_secs(10);
    t.db.expire("key".into(), sooner, Some("LT".into())).unwrap();

    assert_eq!(t.get_string("key"), "v");
}

#[test]
fn expire_on_missing_key_returns_error() {
    let t = TestDb::new();
    let future = Instant::now() + Duration::from_secs(30);
    assert!(t.db.expire("ghost".into(), future, None).is_err());
}
