use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};
use rand::prelude::*;
use tokio;

use anyhow::{Error, Result};

#[derive(Clone)]
pub enum RedisObject {
    String(String),
    List(Vec<RedisObject>),
    // Stream,
    // SortedSet,
}

#[derive(Clone)]
pub struct DB {
    state: Arc<Mutex<State>>,
}
// Arc -> shared ownership
// Mutex -> thread safe mutation
//          (ensures only one thread mutates the state at a time)

pub struct State {
    data: HashMap<String, RedisObject>,
    expirations: HashMap<String, Instant>,
}

impl DB {
    pub fn new() -> DB {
        let db = DB {
            state: Arc::new(Mutex::new(State {
                data: HashMap::new(),
                expirations: HashMap::new(),
            })),
        };

        let db_clone = db.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = db_clone.active_expiration_cycle() {
                    eprintln!("Error running active expiration: {:?}", e);
                }
    
                // running active expiration cycle every 3 sec.
                tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
            }
        });

        db
    }

    pub fn set(&self, key: String, val: RedisObject, expires_at: Option<Instant>) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        // if the key is replacing with a new value remove it's its expirations too.
        if state.expirations.contains_key(&key) {
            state.expirations.remove(&key);
        }

        state.data.insert(key.clone(), val);
        match expires_at {
            Some(exp) => state.expirations.insert(key.clone(), exp),
            None => state.expirations.remove(&key),
        };
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
            Some(ro) => {
                if self.is_expired(&state, &key).unwrap() {
                    state.data.remove(&key);
                    return Ok(RedisObject::String("nil".to_string()));
                };

                Ok(ro.clone())
            },
            None => Ok(RedisObject::String("nil".to_string())),
        }
    }
    
    pub fn lpush(&self, key: String, values: Vec<RedisObject>) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        
        if !state.data.contains_key(&key) {
            state.data.insert(key.clone(), RedisObject::List(values));
        } else {
            match state.data.get(&key) {
                Some(RedisObject::List(list)) => {
                    let mut l = list.clone();
                    for val in values {
                        l.insert(0, val);
                    }
                    state.data.insert(key.clone(), RedisObject::List(l));
                }
                Some(RedisObject::String(_)) => return Err(anyhow::anyhow!("Wrong data type. Expected List, got String.")),
                None => {
                    return Err(anyhow::anyhow!("Entry not found."));
                }
            };
            
        }
        
        Ok(())
    }

    pub fn rpush(&self, key: String, values: Vec<RedisObject>) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        
        if !state.data.contains_key(&key) {
            state.data.insert(key.clone(), RedisObject::List(values));
        } else {
            match state.data.get_mut(&key) {
                Some(RedisObject::List(list)) => {
                    list.extend(values);
                }
                Some(RedisObject::String(_)) => return Err(anyhow::anyhow!("Wrong data type. Expected List, got String.")),
                None => {
                    return Err(anyhow::anyhow!("Entry not found."));
                }
            };
            
        }
        
        Ok(())
    }
    
    pub fn lrange(&self, key: String, mut start: i32, mut stop: i32) -> Result<RedisObject, Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };
        
        match state.data.get(&key) {
            Some(ro) => {
                if self.is_expired(&state, &key).unwrap() {
                    state.data.remove(&key);
                    return Ok(RedisObject::String("nil".to_string()));
                };
    
                if let RedisObject::List(list) = ro {
                    // negetive start and stop represents the index from end
                    if start < 0 {
                        start += list.len() as i32;
                    }
                    if stop < 0 {
                        stop += list.len() as i32;
                    }

                    let vals: Vec<RedisObject> = list.iter()
                        .enumerate()
                        .filter(|(idx, _)| *idx >= start as usize && *idx <= stop as usize)
                        .map(|(_, v)| v.clone())
                        .collect();
                    return Ok(RedisObject::List(vals));
                } else {
                    Err(anyhow::anyhow!("Wrong type. Expected list."))
                }
            },
            None => Ok(RedisObject::String("nil".to_string())),
        }
    }

    pub fn expire(&self, key: String, expires_at: Instant, option: Option<String>) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        if !state.data.contains_key(&key) {
            return Err(anyhow::anyhow!("Entry not found."));
        };

        let update_expiry= match option {
            Some(opt) => match opt.as_str() {
                "NX" => {
                    !state.expirations.contains_key(&key)
                },
                "XX" => {
                    state.expirations.contains_key(&key)
                },
                "GT" => {
                    match state.expirations.get(&key) {
                        Some(existing_expiry) => expires_at > *existing_expiry,
                        None => false,
                    }
                },
                "LT" => {
                    match state.expirations.get(&key) {
                        Some(existing_expiry) => expires_at < *existing_expiry,
                        None => false,
                    }
                },
                _ => return Err(anyhow::anyhow!("Invalid option for expire command.")),
            },
            None => true,
        };

        if update_expiry {
            state.expirations.insert(key, expires_at);
        }

        Ok(())
    }

    // Expirations funcs

    fn is_expired(&self, state: &State, key: &String) -> Result<bool, Error> {
        if let Some(expires_at) = state.expirations.get(key) {
            return Ok(expires_at < &Instant::now());
        }

        Ok(false)
    }

    fn active_expiration_cycle(&self) -> Result<(), Error> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to acquire DB lock. E: {}", e))
            },
        };

        let sample_size = 20;
        let mut expired = 0;

        // getting keys in every active expiration cycle is a good design choice
        // will be improving it later.
        let keys: Vec<String> = state.expirations.keys().cloned().collect();

        if keys.is_empty() {
            return Ok(());
        }

        let mut rng = rand::rng();

        for _ in 0..sample_size {
            let n = rng.random::<u32>() as usize;
            let idx = n % keys.len();
            let k = &keys[idx];

            if self.is_expired(&state, k).unwrap() {
                state.data.remove(k);
                state.expirations.remove(k);
                expired += 1;
            }
        }

        if (expired as f64 / sample_size as f64) > 0.25 {
            return Ok(self.active_expiration_cycle().unwrap());
        }

        Ok(())
    }
}
