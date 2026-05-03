use crate::command::command::Command;

#[derive(Clone)]
pub struct Client {
    pub in_transaction: bool,
    pub queued_commands: Vec<Command>,
}

impl Client {
    pub fn new() -> Client {
        Client {
            in_transaction: false,
            queued_commands: Vec::new(),
        }
    }
}
