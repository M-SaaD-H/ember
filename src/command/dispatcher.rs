use anyhow::Error;

use crate::command::command::Command;
use crate::config::client::Client;

// Dispatch the commands (execute the command)
pub fn dispatch(client: Client, cmd: Command) -> Result<String, Error> {
    match cmd {
        Command::Ping => {
            Ok("Pong".to_string())
        }
        Command::Echo(message) => {
            Ok(message)
        }
        Command::Set(key, value) => {
            match client.set(key, value) {
                Ok(()) => Ok("Ok".to_string()),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
        Command::Get(key) => {
            match client.get(key) {
                Ok(val) => Ok(val),
                Err(e) => Err(anyhow::anyhow!("Failed to execute command. E: {}", e)),
            }
        }
    }
}
