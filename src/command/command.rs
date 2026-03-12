use anyhow::{Error, Result};

use crate::resp::types::RespType;

#[derive(Debug, Clone)]
pub enum Command {
    Ping,
    Echo(String),
    Set(String, String),
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
                Ok(Command::Set(k.clone(), v.clone()))
            } else {
                Err(anyhow::anyhow!("SET arguments must be bulk strings."))
            }
        }
        "GET" => {
            if let RespType::BulkString(arg) = &args[0] {
                Ok(Command::Get(arg.clone()))
            } else {
                Err(anyhow::anyhow!("GET command requires an argument."))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown command.")),
    }
}
