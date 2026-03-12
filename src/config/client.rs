use anyhow::Error;

use crate::database::core::{DB, RedisObject};

pub struct Client {
    db: DB,
}

impl Client {
    pub fn new(db: DB) -> Client {
        Client { db }
    }

    pub fn set(&self, k: String, v: String) -> Result<(), Error> {
        self.db.set(k, RedisObject::String(v))
    }

    pub fn get(&self, k: String) -> Result<String, Error> {
        self.db.get(k)
    }
}
