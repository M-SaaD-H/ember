use anyhow::Error;

use crate::command::command::Command;
use crate::config::client::Client;
use crate::database::core::RedisObject;
use crate::resp::types::RespType;

// Dispatch the commands
pub fn dispatch(client: &mut Client, cmd: Command) -> Result<RespType, Error> {
    // check for transaction
    if client.in_transaction {
        if matches!(cmd, Command::EXEC | Command::DISCARD) {
            return execute_command(client, cmd);
        }

        client.queued_commands.push(cmd);
        return Ok(RespType::SimpleString(String::from("QUEUED")));
    }

    execute_command(client, cmd)
}

fn execute_command(client: &mut Client, cmd: Command) -> Result<RespType, Error> {
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
            match client.db.set(key, val, expires_in) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::GET(key) => {
            match client.db.get(key) {
                Ok(RedisObject::String(s)) => Ok(RespType::BulkString(s)),
                Ok(RedisObject::List(_)) => Err(anyhow::anyhow!("Wrong data type. Expected String, got List.")),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::EXPIRE(key, expires_at, option) => {
            match client.db.expire(key, expires_at, option) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::LPUSH(key, vals) => {
            let values = vals.iter().map(|v| RedisObject::String(v.clone())).collect();
            match client.db.lpush(key, values) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::RPUSH(key, vals) => {
            let values = vals.iter().map(|v| RedisObject::String(v.clone())).collect();
            match client.db.rpush(key, values) {
                Ok(()) => Ok(RespType::SimpleString("Ok".to_string())),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::LRANGE(key, start, stop ) => {
            match client.db.lrange(key, start, stop) {
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
                Ok(_) => Err(anyhow::anyhow!("Unexpected Error: expected list")),
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
                match execute_command(client, c) {
                    Ok(rep) => replies.push(rep),
                    Err(e) => return Err(anyhow::anyhow!("Failed to execute queued commands. E: {}", e)),
                }
            }
            client.in_transaction = false;
            client.queued_commands.clear();
            Ok(RespType::Array(replies))
        }
    }
}
