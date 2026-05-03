use anyhow::Result;

use crate::command::command::Command;
use crate::config::client::Client;
use crate::database::core::{DB, RedisObject};
use crate::resp::types::RespType;

// Dispatch the commands
pub fn dispatch(client: &mut Client, db: &mut DB, cmd: Command) -> Result<RespType> {
    // check for transaction
    if client.in_transaction {
        if matches!(cmd, Command::MULTI) {
            return Err(anyhow::anyhow!("Can not nest multi"));
        }

        if matches!(cmd, Command::EXEC | Command::DISCARD) {
            return execute_command(client, db, cmd);
        }

        client.queued_commands.push(cmd);
        return Ok(RespType::SimpleString(String::from("QUEUED")));
    }

    execute_command(client, db, cmd)
}

fn execute_command(client: &mut Client, db: &mut DB, cmd: Command) -> Result<RespType> {
    // execute commands
    match cmd {
        Command::PING => {
            Ok(RespType::SimpleString("Pong".to_string()))
        }
        Command::ECHO(message) => {
            Ok(RespType::BulkString(message))
        }
        Command::SET(key, value, expires_in) => {
            let val = RedisObject::String(value);
            if let Some(exp) = expires_in {
                println!("exp: {:?}", exp);
            }
            match db.set(key, val, expires_in) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::GET(key) => {
            match db.get(key) {
                Ok(RedisObject::String(s)) => Ok(RespType::BulkString(s)),
                Ok(RedisObject::List(_)) => Err(anyhow::anyhow!("Wrong data type. Expected String, got List.")),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::DELETE(key) => {
            match db.delete(key) {
                Ok(()) => Ok(RespType::BulkString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::EXPIRE(key, expires_at, option) => {
            match db.expire(key, expires_at, option) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::LPUSH(key, vals) => {
            let values = vals.iter().map(|v| RedisObject::String(v.clone())).collect();
            match db.lpush(key, values) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::RPUSH(key, vals) => {
            let values = vals.iter().map(|v| RedisObject::String(v.clone())).collect();
            match db.rpush(key, values) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::LRANGE(key, start, stop ) => {
            match db.lrange(key, start, stop) {
                Ok(RedisObject::List(list)) => {
                    Ok(RespType::Array(
                        list.iter().filter_map(|item| {
                            if let RedisObject::String(s) = item {
                                Some(RespType::BulkString(s.clone()))
                            } else {
                                None
                            }
                        }).collect()
                    ))
                },
                Ok(RedisObject::String(s)) => Ok(RespType::BulkString(s)),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }

        Command::MULTI => {
            client.in_transaction = true;
            client.queued_commands.clear();

            Ok(RespType::SimpleString("Ok".to_string()))
        }
        Command::DISCARD => {
            client.in_transaction = false;
            client.queued_commands.clear();

            Ok(RespType::SimpleString("Ok".to_string()))
        }
        Command::EXEC => {
            let mut replies: Vec<RespType> = Vec::new();
            let queued_cmds = client.queued_commands.clone();
            for c in queued_cmds {
                match execute_command(client, db, c) {
                    Ok(rep) => replies.push(rep),
                    Err(e) => return Err(anyhow::anyhow!("Failed to execute queued commands. E: {}", e)),
                }
            }
            client.in_transaction = false;
            client.queued_commands.clear();
            Ok(RespType::Array(replies))
        }

        Command::SAVE => {
            match db.save_rdb() {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(_) => Err(anyhow::anyhow!("Error while saving rdb snapshot.")),
            }
        }
    }
}
