use std::{collections::HashMap, fs::File, io::Read};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::database::core::RedisObject;

fn load_rdb(file_path: &str) -> Result<(
    HashMap<String, RedisObject>,
    HashMap<String, Instant>
)> {
    let mut file = File::open(file_path)?;

    Ok(parse(&mut file)?)
}

fn parse(file: &mut File) -> Result<(
    HashMap<String, RedisObject>,
    HashMap<String, Instant>
)> {
    
    read_header(file)?;
    
    let mut db: HashMap<String, RedisObject> = HashMap::new();
    let mut expirations: HashMap<String, Instant> = HashMap::new();

    loop {
        let opcode = match read_byte(file) {
            Ok(b) => b,
            Err(_) => break,
        };

        match opcode {
            0xFE => {
                let _db = read_length(file)?; // ignore for now
            }

            0xFC => {
                let key = read_string(file)?;
                let expiry = read_expiry(file)?;
                expirations.insert(key, expiry);
            }

            0x00 => {
                let data_type = read_string(file)?;
                
                match data_type.to_ascii_uppercase().as_str() {
                    "STRING" => {
                        let key = read_string(file)?;
                        let value = read_string(file)?;
                        db.insert(
                            key,
                            RedisObject::String(value),
                        );
                    },
                    "LIST" => {
                        let length = read_length(file)?;
                        for _ in 0..length {
                            let key = read_string(file)?;
                            let value = read_string(file)?;
                            db.insert(
                                key,
                                RedisObject::String(value),
                            );
                        }
                    }
                    _ => return Err(anyhow::anyhow!("Unknown data type")),
                }
            }

            0xFF => break,

            _ => {
                return Err(anyhow::anyhow!("Unknown opcode"));
            }
        }
    }

    Ok((db, expirations))
}

fn read_header(file: &mut File) -> Result<()> {
    let mut buf = [0u8; 9];
    file.read_exact(&mut buf)?;

    if &buf[..5] != b"REDIS" {
        return Err(anyhow::anyhow!("Invalid header."));
    }

    Ok(())
}

fn read_byte(file: &mut File) -> Result<u8> {
    let mut buf = [0u8; 1];
    file.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_expiry(file: &mut File) -> Result<Instant> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;

    let ms = u64::from_le_bytes(buf);
    let target_system_time = UNIX_EPOCH + Duration::from_millis(ms);

    // Convert SystemTime to Instant
    let now_instant = Instant::now();
    let now_system = SystemTime::now();

    let duration = match target_system_time.duration_since(now_system) {
        Ok(dur) => dur,
        Err(_) => Duration::from_secs(0), // Already expired
    };

    Ok(now_instant + duration)
}

fn read_string(file: &mut File) -> Result<String> {
    let len = read_length(file)? as usize;

    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;

    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn read_length(file: &mut File) -> Result<u64> {
    let first = read_byte(file)?;

    if first < 64 {
        Ok(first as u64)
    } else {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf) as u64)
    }
}
