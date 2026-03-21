use anyhow::Error;

use crate::command::command::Command;
use crate::config::client::Client;
use crate::database::core::{Entry, RedisObject};
use crate::resp::types::RespType;

// Dispatch the commands (execute the command)
pub fn dispatch(client: Client, cmd: Command) -> Result<RespType, Error> {
    match cmd {
        Command::Ping => {
            Ok(RespType::SimpleString("Pong".to_string()))
        }
        Command::Echo(message) => {
            Ok(RespType::BulkString(message))
        }
        Command::Set(key, value, expires_at) => {
            let e = Entry::new(RedisObject::String(value), expires_at);
            match client.db.set(key, e) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::Get(key) => {
            match client.db.get(key) {
                // Ok(entry) => Ok(RespType::BulkString(val)),
                Ok(entry) => {
                    match entry.expires_at {
                        Some(ex) => {
                            println!("ex: {:?}", ex);
                        },
                        None => {
                            println!("No ex");
                        }
                    }
                    match entry.value {
                        RedisObject::String(s) => Ok(RespType::BulkString(s)),
                    }
                }
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
    }
}
