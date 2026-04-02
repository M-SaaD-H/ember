use std::time::{Duration, Instant};

use anyhow::{Error, Result};

use crate::resp::types::RespType;

#[derive(Debug, Clone)]
pub enum Command {
    Ping,
    Echo(String),                                // (msg)
    Set(String, String, Option<Instant>),        // (key, val, Option<expires_in>)
    Get(String),                                 // (key)

    LPush(String, Vec<String>),                  // (key, values)
    RPush(String, Vec<String>),                  // (key, values)
    LRange(String, i32, i32),                    // (key, start, stop)
    Expire(String, Instant, Option<String>),     // (key, expires_in, Option<NX | XX | GT | LT>)
}

// Extract commands from the user input
pub fn extract_command(resp: &RespType) -> Result<Command, Error> {
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
fn parse_command(cmd: &str, args: &[RespType]) -> Result<Command, Error> {
    match cmd {
        "PING" => Ok(Command::Ping),
        "ECHO" => {
            if let RespType::BulkString(arg) = &args[0] {
                Ok(Command::Echo(arg.clone()))
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
                    return Ok(Command::Set(k.clone(), v.clone(), None));
                }

                let expires_in_millis =
                    if let (
                        RespType::BulkString(flag),
                        RespType::Integer(expires_in)
                    ) = (
                        &args[2], &args[3]
                    ) {
                        if flag.to_ascii_uppercase() == "EX" {
                            Some(*expires_in * 1000)
                        } else if flag.to_ascii_uppercase() == "PX" {
                            Some(*expires_in)
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

                Ok(Command::Set(k.clone(), v.clone(), expires_at))
            } else {
                Err(anyhow::anyhow!("SET arguments must be bulk strings."))
            }
        }
        "GET" => {
            if let RespType::BulkString(k) = &args[0] {
                Ok(Command::Get(k.clone()))
            } else {
                Err(anyhow::anyhow!("GET command requires an argument."))
            }
        }
        "LPUSH" => {
            if let RespType::BulkString(k) = &args[0] {
                if let RespType::BulkString(v) = &args[1] {
                    Ok(Command::LPush(k.clone(), vec![v.clone()]))
                } else if let RespType::Array(values) = &args[1] {
                    Ok(Command::LPush(
                        k.clone(),
                        values.iter().filter_map(|v| {
                            if let RespType::BulkString(bs) = v {
                                Some(bs.clone())
                            } else {
                                None
                            }
                        }).collect(),
                    ))
                } else {
                    Err(anyhow::anyhow!("LPUSH command requires an array of values."))
                }
            } else {
                Err(anyhow::anyhow!("LPUSH command requires an argument."))
            }
        }
        "RPUSH" => {
            if let RespType::BulkString(k) = &args[0] {
                if let RespType::BulkString(v) = &args[1] {
                    Ok(Command::RPush(k.clone(), vec![v.clone()]))
                } else if let RespType::Array(values) = &args[1] {
                    Ok(Command::RPush(
                        k.clone(),
                        values.iter().filter_map(|v| {
                            if let RespType::BulkString(bs) = v {
                                Some(bs.clone())
                            } else {
                                None
                            }
                        }).collect(),
                    ))
                } else {
                    Err(anyhow::anyhow!("RPUSH command requires an array of values."))
                }
            } else {
                Err(anyhow::anyhow!("RPUSH command requires an argument."))
            }
        }
        "LRANGE" => {
            if let (
                RespType::BulkString(key),
                RespType::Integer(start),
                RespType::Integer(stop),
            ) = (
                &args[0],
                &args[1],
                &args[2],
            ) {
                Ok(Command::LRange(key.clone(), start.clone(), stop.clone()))
            } else {    
                Err(anyhow::anyhow!("LPUSH command requires an argument."))
            }
        }
        "EXPIRE" => {
            if let (
                RespType::BulkString(k),
                RespType::Integer(expires_in),
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

                let expires_at = Instant::now() + Duration::from_millis(*expires_in as u64);

                Ok(Command::Expire(k.clone(), expires_at.clone(), option))
            } else {
                Err(anyhow::anyhow!("EXPIRE command requires an argument."))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown command.")),
    }
}
