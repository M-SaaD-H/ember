use std::{sync::Arc, time::Instant};
use dashmap::DashMap;
use rand::prelude::*;

use anyhow::Result;

use crate::rdb::{reader::load_rdb, writer::save_rdb};

pub const RDB_FILE: &str = "snapshots/dump.rdb";

#[derive(Clone, Debug, PartialEq)]
pub enum RedisObject {
    String(String),
    List(Vec<RedisObject>),
    // Stream,
    // SortedSet,
}

// DB is a cheaply-cloneable handle to the shared server state.
// Internally it holds two Arc<DashMap<...>> — one for data, one for
// expiration timestamps.
//
// DashMap shards the key-space across 64 independent RwLocks, so
// concurrent reads/writes to different keys never contend at all.
// This replaces the previous Arc<Mutex<State>> which serialised every
// GET, SET, DELETE etc. behind a single lock.
// 
// DashMap = high-performance, concurrent HashMap
// It handles internal locking automatically
// It uses sharding. Instead of a single for the entire map, it splits
// the data into multiple shards each with it's own

#[derive(Clone, Debug)]
pub struct DB {
    data: Arc<DashMap<String, RedisObject>>,
    expirations: Arc<DashMap<String, Instant>>,
}

impl DB {
    /// Creates a fresh in-memory [`DB`] without loading from disk or spawning
    /// the background expiration task.
    ///
    /// **Intended for use in tests only.** Do not call in production code.
    #[allow(dead_code)]
    pub fn new_in_memory() -> DB {
        DB {
            data: Arc::new(DashMap::new()),
            expirations: Arc::new(DashMap::new()),
        }
    }

    pub fn new() -> DB {
        let (data_map, exp_map) = load_rdb(RDB_FILE).unwrap();

        let db = DB {
            data: Arc::new(DashMap::from_iter(data_map)),
            expirations: Arc::new(DashMap::from_iter(exp_map)),
        };

        // Spawn the active-expiration background task. It holds the DashMap
        // shard locks only for the nanoseconds it takes to remove a single
        // entry - it never holds any lock across a sleep.
        let db_clone = db.clone();
        tokio::spawn(async move {
            db_clone.active_expiration_task().await;
        });

        db
    }

    pub fn set(&self, key: String, val: RedisObject, expires_at: Option<Instant>) -> Result<()> {
        // Removing the old expiration (if any) before inserting the new value
        // keeps the two maps in sync regardless of whether an expiry is set.
        self.expirations.remove(&key);

        self.data.insert(key.clone(), val);

        match expires_at {
            Some(exp) => { self.expirations.insert(key, exp); }
            None => { /* no expiration, nothing to do */ }
        }

        Ok(())
    }

    pub fn get(&self, key: String) -> Result<RedisObject> {
        // Lazy expiration: check the expiration map before returning the value.
        // The DashMap shard lock for 'key' is held only for the duration of
        // the .get() call (a few nanoseconds), not for our entire function.
        if let Some(exp) = self.expirations.get(&key) {
            if self.is_expired(&key) {
                // Key has expired, drop the read reference before mutating
                drop(exp);

                self.data.remove(&key);
                self.expirations.remove(&key);
                return Ok(RedisObject::String("nil".to_string()));
            }
        }

        match self.data.get(&key) {
            Some(ro) => Ok(ro.clone()),
            None     => Ok(RedisObject::String("nil".to_string())),
        }
    }

    pub fn delete(&self, key: String) -> Result<()> {
        self.data.remove(&key);
        self.expirations.remove(&key);
        Ok(())
    }

    pub fn lpush(&self, key: String, values: Vec<RedisObject>) -> Result<()> {
        match self.data.get_mut(&key) {
            None => {
                self.data.insert(key, RedisObject::List(values));
            }
            Some(mut entry) => match entry.value_mut() {
                RedisObject::List(list) => {
                    // LPUSH inserts at the head, so prepend in order.
                    let mut new_head = values;
                    new_head.extend(list.drain(..));
                    *list = new_head;
                }
                RedisObject::String(_) => {
                    return Err(anyhow::anyhow!("Wrong data type. Expected List, got String."));
                }
            },
        }
        Ok(())
    }

    pub fn rpush(&self, key: String, values: Vec<RedisObject>) -> Result<()> {
        match self.data.get_mut(&key) {
            None => {
                self.data.insert(key, RedisObject::List(values));
            }
            Some(mut entry) => match entry.value_mut() {
                RedisObject::List(list) => {
                    list.extend(values);
                }
                RedisObject::String(_) => {
                    return Err(anyhow::anyhow!("Wrong data type. Expected List, got String."));
                }
            },
        }
        Ok(())
    }

    pub fn lrange(&self, key: String, mut start: i32, mut stop: i32) -> Result<RedisObject> {
        // Lazy expiration check.
        if let Some(exp) = self.expirations.get(&key) {
            if self.is_expired(&key) {
                drop(exp);

                self.data.remove(&key);
                self.expirations.remove(&key);
                return Ok(RedisObject::String("nil".to_string()));
            }
        }

        match self.data.get(&key) {
            None => Ok(RedisObject::String("nil".to_string())),
            Some(ro) => {
                if let RedisObject::List(list) = ro.value() {
                    // Negative indices count from the end.
                    if start < 0 {
                        start += list.len() as i32;
                    }
                    if stop < 0 {
                        stop += list.len() as i32;
                    }

                    let vals: Vec<RedisObject> = list
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| *idx >= start as usize && *idx <= stop as usize)
                        .map(|(_, v)| v.clone())
                        .collect();

                    Ok(RedisObject::List(vals))
                } else {
                    Err(anyhow::anyhow!("Wrong type. Expected list."))
                }
            }
        }
    }

    pub fn expire(&self, key: String, expires_at: Instant, option: Option<String>) -> Result<()> {
        if !self.data.contains_key(&key) {
            return Err(anyhow::anyhow!("Entry not found."));
        }

        let update_expiry = match option {
            Some(opt) => match opt.as_str() {
                "NX" => !self.expirations.contains_key(&key),
                "XX" => self.expirations.contains_key(&key),
                "GT" => self.expirations
                            .get(&key)
                            .map_or(false, |e| expires_at > *e),
                "LT" => self.expirations
                            .get(&key)
                            .map_or(false, |e| expires_at < *e),
                _ => return Err(anyhow::anyhow!("Invalid option for expire command.")),
            },
            None => true,
        };

        if update_expiry {
            self.expirations.insert(key, expires_at);
        }

        Ok(())
    }

    // Expiration

    fn is_expired(&self, key: &String) -> bool {
        if let Some(expires_at) = self.expirations.get(key) {
            return *expires_at < Instant::now();
        }

        false
    }

    // Runs forever as a background task, waking every 100 ms to purge expired
    // keys. Each cycle is a short burst of work; the task yields to the Tokio
    // scheduler between cycles so it never starves I/O tasks.
    async fn active_expiration_task(&self) {
        loop {
            self.active_expiration_cycle();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    // Samples up to 20 keys from the expiration map and removes any that have
    // expired. If more than 25 % of the sample was expired, the cycle runs
    // again immediately (Redis's algorithm).
    //
    // Crucially, each DashMap operation holds a shard lock for nanoseconds
    // only, there is no monolithic lock held across the whole function, so
    // live GET/SET traffic is never blocked by this background task.
    fn active_expiration_cycle(&self) {
        let sample_size = 20usize;
        let now = Instant::now();
        let mut rng = rand::rng();

        // Collect up to `sample_size` random keys from the expirations map.
        // We materialise them into a Vec so we release the iterator (and its
        // shard locks) before calling remove().
        let sampled: Vec<String> = {
            let total = self.expirations.len();
            if total == 0 {
                return;
            }
            let skip = rng.random::<u64>() as usize % total;
            self.expirations
                .iter()
                .skip(skip)
                .take(sample_size)
                .map(|e| e.key().clone())
                .collect()
        };

        let mut expired = 0usize;
        for key in &sampled {
            if let Some(exp) = self.expirations.get(key) {
                if *exp < now {
                    // Drops the reference to the expiration value before mutating the DashMap.
                    drop(exp);
            
                    self.data.remove(key);
                    self.expirations.remove(key);
                    expired += 1;
                }
            }
        }

        // Redis re-runs the cycle immediately when the expired ratio is high,
        // so we keep purging without waiting for the next 100 ms tick.
        let checked = sampled.len().max(1);
        if (expired as f64 / checked as f64) > 0.25 {
            self.active_expiration_cycle();
        }
    }

    // RDB

    // Use child processes to optimize snapshot writes further 
    pub fn save_rdb(&self) -> Result<()> {
        // Take a point-in-time snapshot of both maps. The clones are Arc
        // clones under the hood (O(n) for the entries, but lock-free per
        // shard). We do this quickly and then hand the snapshot off to a
        // background task so we never block the I/O event loop.
        let data_snapshot: std::collections::HashMap<String, RedisObject> =
            self.data
                .iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect();
        let exp_snapshot: std::collections::HashMap<String, Instant> =
            self.expirations
                .iter()
                .map(|e| (e.key().clone(), *e.value()))
                .collect();

        tokio::spawn(async move {
            if let Err(e) = save_rdb(RDB_FILE, &data_snapshot, &exp_snapshot) {
                eprintln!("Error while writing rdb. E: {}", e);
            }
        });

        Ok(())
    }
}
