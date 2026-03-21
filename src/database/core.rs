use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

use anyhow::{Error, Result};

#[derive(Clone)]
pub enum RedisObject {
    String(String),
    // Add these later
    // List(Vec<RedisObject>),
    // Stream,
    // SortedSet,
}

#[derive(Clone)]
pub struct Entry {
    pub value: RedisObject,
    pub expires_at: Option<Instant>, // in millis
}

impl Entry {
    pub fn new(value: RedisObject, expires_at: Option<Instant>) -> Entry {
        Entry {
            value,
            expires_at
        }
    }
}

#[derive(Clone)]
pub struct DB {
    data: Arc<Mutex<HashMap<String, Entry>>>
}

impl DB {
    pub fn new() -> DB {
        DB {
            data: Arc::new(Mutex::new(HashMap::<String, Entry>::new())),
        }
    }

    pub fn set(&self, key: String, entry: Entry) -> Result<(), Error> {
        let mut state = match self.data.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        state.insert(key, entry);
        Ok(())
    }

    pub fn get(&self, key: String) -> Result<Entry, Error> {
        let state = match self.data.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        match state.get(&key) {
            // Some(RedisObject::String(val)) => Ok(val.clone()),
            Some(e) => Ok(e.clone()),
            None => Ok(Entry {
                value: RedisObject::String("nil".to_string()),
                expires_at: None
            }),
        }
    }
}
