use anyhow::Error;

use crate::command::command::Command;
use crate::config::client::Client;
use crate::database::core::RedisObject;
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
        Command::Set(key, value, expires_in) => {
            let val = RedisObject::String(value);
            match client.db.set(key, val, expires_in) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::Get(key) => {
            match client.db.get(key) {
                Ok(RedisObject::String(s)) => Ok(RespType::BulkString(s)),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::Expire(key, expires_at, option) => {
            match client.db.expire(key, expires_at, option) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
    }
}
