use crate::command::command::Command;
use crate:: database::core::DB;

#[derive(Clone)]
pub struct Client {
    pub db: DB,
    pub in_transaction: bool,
    pub queued_commands: Vec<Command>,
}

impl Client {
    pub fn new(db: DB, in_transaction: bool, queued_commands: Vec<Command>) -> Client {
        Client {
            db,
            in_transaction,
            queued_commands,
        }
    }
}
