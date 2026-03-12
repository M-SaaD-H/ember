use std::{collections::HashMap, sync::{Arc, Mutex}};

use anyhow::{Error, Result};

pub enum RedisObject {
    String(String),
    // Add these later
    // List(Vec<RedisObject>),
    // Stream,
    // SortedSet,
}

#[derive(Clone)]
pub struct DB {
    data: Arc<Mutex<HashMap<String, RedisObject>>>
}

impl DB {
    pub fn new() -> DB {
        DB {
            data: Arc::new(Mutex::new(HashMap::<String, RedisObject>::new())),
        }
    }

    pub fn set(&self, key: String, val: RedisObject) -> Result<(), Error> {
        let mut state = match self.data.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        state.insert(key, val);
        Ok(())
    }

    pub fn get(&self, key: String) -> Result<String, Error> {
        let state = match self.data.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        match state.get(&key) {
            Some(RedisObject::String(val)) => Ok(val.clone()),
            None => Ok(String::from("nil")),
        }
    }
}
