use crate::command::command::Command;
use crate:: database::core::DB;

#[derive(Clone)]
pub struct Client {
    pub db: DB,
    pub in_transaction: bool,
    pub queued_commands: Vec<Command>,
}

impl Client {
    pub fn new() -> Client {
        Client {
            db: DB::new(),
            in_transaction: false,
            queued_commands: Vec::new(),
        }
    }
}
