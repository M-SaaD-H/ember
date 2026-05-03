use crate::command::command::Command;
use crate:: database::core::DB;

#[derive(Clone)]
pub struct Client {
    pub db: DB,
    pub in_transaction: bool,
    pub queued_commands: Vec<Command>,
    pub rdb_file: String,
}

impl Client {
    pub fn new(client_id: u16) -> Client {
        let rdb_file_path = String::from(format!("snapshots/client-{}.rdb", client_id));
        Client {
            db: DB::new(&rdb_file_path),
            in_transaction: false,
            queued_commands: Vec::new(),
            rdb_file: rdb_file_path,
        }
    }
}
