use crate::database::core::DB;

pub struct Client {
    pub db: DB,
}

impl Client {
    pub fn new() -> Client {
        Client {
            db: DB::new(),
        }
    }
}
