use anyhow::Error;

use crate::command::command::Command;

pub fn dispatch(cmd: Command) -> Result<String, Error> {
    match cmd {
        Command::Ping => {
            Ok("Pong\r\n".to_string())
        }
        Command::Echo(message) => {
            Ok(message + "\r\n")
        }
        Command::Set(_key, _value) => {
            Ok("Ok\r\n".to_string())
        }
        Command::Get(_key) => {
            Ok("Got the key\r\n".to_string())
        }
    }
}