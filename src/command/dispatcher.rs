use anyhow::Error;

use crate::command::command::Command;

// Dispatch the commands (execute the command)
pub fn dispatch(cmd: Command) -> Result<String, Error> {
    match cmd {
        Command::Ping => {
            Ok("Pong".to_string())
        }
        Command::Echo(message) => {
            Ok(message)
        }
        Command::Set(_key, _value) => {
            Ok("Ok".to_string())
        }
        Command::Get(_key) => {
            Ok("key".to_string())
        }
    }
}
