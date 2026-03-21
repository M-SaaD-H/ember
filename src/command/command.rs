use std::time::{Duration, Instant};

use anyhow::{Error, Result};

use crate::resp::types::RespType;

#[derive(Debug, Clone)]
pub enum Command {
    Ping,
    Echo(String),
    Set(String, String, Option<Instant>),
    Get(String),
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

                let expire_int =
                    if let (
                        RespType::BulkString(flag),
                        RespType::BulkString(expires_at)
                    ) = (
                        &args[2], &args[3]
                    ) {
                        if flag.to_ascii_uppercase() == "EX" {
                            match expires_at.parse::<u64>() {
                                Ok(val) => Some(val * 1000),
                                Err(_) => None,
                            }
                        } else if flag.to_ascii_uppercase() == "PX" {
                            match expires_at.parse::<u64>() {
                                Ok(val) => Some(val),
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                } else {
                    None
                };

                let expire = match expire_int {
                    Some(e) => Some(Instant::now() + Duration::from_millis(e)),
                    None => None,
                };

                Ok(Command::Set(k.clone(), v.clone(), expire))
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
        _ => Err(anyhow::anyhow!("Unknown command.")),
    }
}
