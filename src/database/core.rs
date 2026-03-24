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

    pub fn set(&self, key: &String, val: RedisObject, expires_at: Option<Instant>) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        state.data.insert(key.clone(), val);
        match expires_at {
            Some(exp) => state.expirations.insert(key.clone(), exp),
            None => state.expirations.remove(key),
        };
        Ok(())
    }

    pub fn get(&self, key: &String) -> Result<RedisObject, Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        match state.data.get(key) {
            Some(ro) => {
                if self.is_expired(&state, key).unwrap() {
                    state.data.remove(key);
                    return Ok(RedisObject::String("nil".to_string()));
                };

                Ok(ro.clone())
            },
            None => Ok(RedisObject::String("nil".to_string())),
        }
    }

    // Expirations funcs

    fn is_expired(&self, state: &State, key: &String) -> Result<bool, Error> {
        if let Some(expires_at) = state.expirations.get(key) {
            return Ok(expires_at < &Instant::now());
        }

        Ok(false)
    }
}
