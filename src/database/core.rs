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
pub struct DB {
    state: Arc<Mutex<State>>,
}

pub struct State {
    data: HashMap<String, RedisObject>,
    expirations: HashMap<String, Instant>,
}

impl DB {
    pub fn new() -> DB {
        DB {
            state: Arc::new(Mutex::new(State {
                data: HashMap::new(),
                expirations: HashMap::new(),
            })),
        }
    }

    pub fn set(&self, key: String, val: RedisObject) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        state.data.insert(key, val);
        Ok(())
    }

    pub fn get(&self, key: String) -> Result<RedisObject, Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        match state.data.get(&key) {
            Some(e) => {
                if let Some(expires_at) = state.expirations.get(&key) {
                    if expires_at < &Instant::now() {
                        state.data.remove(&key);
                        return Ok(RedisObject::String("nil".to_string()));
                    } else {
                        println!("not expired");
                        return Ok(e.clone());
                    };
                }

                Ok(e.clone())
            },
            None => Ok(RedisObject::String("nil".to_string())),
        }
    }
}
