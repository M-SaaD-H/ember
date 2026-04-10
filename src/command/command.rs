use std::time::{Duration, Instant};

use anyhow::{Result};

use crate::resp::types::RespType;
use crate::utils::parse_int;

#[derive(Debug, Clone)]
pub enum Command {
    PING,
    ECHO(String),                                // (msg)
    SET(String, String, Option<Instant>),        // (key, val, Option<expires_in>)
    GET(String),                                 // (key)
    DELETE(String),                              // (key)

    LPUSH(String, Vec<String>),                  // (key, values)
    RPUSH(String, Vec<String>),                  // (key, values)
    LRANGE(String, i32, i32),                    // (key, start, stop)
    EXPIRE(String, Instant, Option<String>),     // (key, expires_in, Option<NX | XX | GT | LT>)

    MULTI,
    DISCARD,
    EXEC,

    SAVE,
}

// Extract commands from the user input
pub fn extract_command(resp: &RespType) -> Result<Command> {
    match resp {
        RespType::Array(arr) => {
            if arr.is_empty() {
                return Err(anyhow::anyhow!("Invalid command format."));
            }

            let command = match &arr[0] {
                RespType::BulkString(cmd) => cmd.to_ascii_uppercase(),
                _ => return Err(anyhow::anyhow!("Invalid command format.")),
            };

            let args: Vec<RespType> = arr.iter().skip(1).cloned().collect();

            let command_enum = parse_command(&command, &args)?;
            Ok(command_enum)
        }
        _ => Err(anyhow::anyhow!("Invalid command format.")),
    }
}

// Create the command enum for the respective input commands
fn parse_command(cmd: &str, args: &[RespType]) -> Result<Command> {
    match cmd {
        "PING" => Ok(Command::PING),
        "ECHO" => {
            if let RespType::BulkString(arg) = &args[0] {
                Ok(Command::ECHO(arg.clone()))
            } else {
                Err(anyhow::anyhow!("ECHO command requires an argument."))
            }
        }
        "SET" => {
            if args.len() < 2 {
                return Err(anyhow::anyhow!("SET command requires two arguments."));
            }

            if let (RespType::BulkString(k), RespType::BulkString(v)) = (&args[0], &args[1]) {
                // no expiry
                if args.len() < 4 {
                    return Ok(Command::SET(k.clone(), v.clone(), None));
                }

                let expires_in_millis =
                    if let (
                        RespType::BulkString(flag),
                        RespType::BulkString(expires_in)
                    ) = (
                        &args[2], &args[3]
                    ) {
                        let expires_in_int = parse_int(expires_in);

                        if flag.to_ascii_uppercase() == "EX" {
                            Some(expires_in_int * 1000)
                        } else if flag.to_ascii_uppercase() == "PX" {
                            Some(expires_in_int)
                        } else {
                            None
                        }
                } else {
                    None
                };

                let expires_at = match expires_in_millis {
                    Some(e) => Some(Instant::now() + Duration::from_millis(e as u64)),
                    None => None,
                };

                Ok(Command::SET(k.clone(), v.clone(), expires_at))
            } else {
                Err(anyhow::anyhow!("SET arguments must be bulk strings."))
            }
        }
        "GET" => {
            if let RespType::BulkString(k) = &args[0] {
                Ok(Command::GET(k.clone()))
            } else {
                Err(anyhow::anyhow!("GET command requires an argument."))
            }
        }
        "DELETE" => {
            if let RespType::BulkString(k) = &args[0] {
                Ok(Command::DELETE(k.clone()))
            } else {
                Err(anyhow::anyhow!("DELETE command requires an argument."))
            }
        }
        "LPUSH" => {
            if let RespType::BulkString(k) = &args[0] {
                let mut vec: Vec<String> = Vec::new();

                for i in 1..args.len() {
                    if let RespType::BulkString(bs) = &args[i] {
                        vec.push(bs.clone());
                    }
                }

                Ok(Command::LPUSH(k.clone(), vec))
            } else {
                Err(anyhow::anyhow!("LPUSH command requires an argument."))
            }
        }
        "RPUSH" => {
            if let RespType::BulkString(k) = &args[0] {
                let mut vec: Vec<String> = Vec::new();

                for i in 1..args.len() {
                    if let RespType::BulkString(bs) = &args[i] {
                        vec.push(bs.clone());
                    }
                }

                Ok(Command::RPUSH(k.clone(), vec))
            } else {
                Err(anyhow::anyhow!("RPUSH command requires an argument."))
            }
        }
        "LRANGE" => {
            if let (
                RespType::BulkString(key),
                RespType::BulkString(start),
                RespType::BulkString(stop),
            ) = (
                &args[0],
                &args[1],
                &args[2],
            ) {
                let start_int = parse_int(start);
                let stop_int = parse_int(stop);
                Ok(Command::LRANGE(key.clone(), start_int, stop_int))
            } else {    
                Err(anyhow::anyhow!("LRANGE command requires an argument."))
            }
        }
        "EXPIRE" => {
            if let (
                RespType::BulkString(k),
                RespType::BulkString(expires_in),
            ) = (
                &args[0],
                &args[1]
            ) {
                let option: Option<String> =
                    if args.len() > 2 {
                        if let RespType::BulkString(opt) = &args[2] {
                            match opt.as_str() {
                                "NX" | "XX" | "GT" | "LT" => Some(opt.to_string()),
                                _ => return Err(anyhow::anyhow!("Invalid option in expire command. Only NX, XX, GT, LT are supported.")),
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                let expires_in_int = parse_int(expires_in);

                let expires_at = Instant::now() + Duration::from_millis(expires_in_int as u64);

                Ok(Command::EXPIRE(k.clone(), expires_at.clone(), option))
            } else {
                Err(anyhow::anyhow!("EXPIRE command requires an argument."))
            }
        }
        
        "MULTI" => {
            Ok(Command::MULTI)
        }
        "DISCARD" => {
            Ok(Command::DISCARD)
        }
        "EXEC" => {
            Ok(Command::EXEC)
        }

        "SAVE" => {
            Ok(Command::SAVE)
        }
        
        // this is for redis-cli as it sends the default command as "COMMAND DOCS".
        "COMMAND" => Ok(Command::PING),
        _ => Err(anyhow::anyhow!("Unknown command.")),
    }
}
