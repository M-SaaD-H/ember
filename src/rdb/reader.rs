use std::path::Path;
use std::{collections::HashMap, fs::File, io::Read};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::database::core::RedisObject;

pub fn load_rdb(file_path: &str) -> Result<(
    HashMap<String, RedisObject>,
    HashMap<String, Instant>
)> {
    // no existing rdb file exists.
    // return empty data and expirations
    if !Path::new(file_path).exists() {
        return Ok((HashMap::new(), HashMap::new()));
    }
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
        let marker = match read_byte(file) {
            Ok(b) => b,
            Err(_) => break,
        };

        match marker {
            0xFE => {
                let _db = read_length(file)?; // ignore for now
            }

            0xFC => {
                let key = read_string(file)?;
                let expiry = read_expiry(file)?;
                expirations.insert(key, expiry);
            }

            0xFF => break,

            _ => {
                // Writer encodes entries as:
                // <type-string> then payload.
                // It also encodes expirations as:
                // <key-string> 0xFC <u64-ms-le>.
                let token = read_string_with_first_len(file, marker)?;

                match token.to_ascii_uppercase().as_str() {
                    "STRING" => {
                        let value_type = read_byte(file)?;
                        if value_type != 0x00 {
                            return Err(anyhow::anyhow!(
                                "Unexpected value type marker for STRING: {}",
                                value_type
                            ));
                        }

                        let key = read_string(file)?;
                        let value = read_string(file)?;
                        db.insert(key, RedisObject::String(value));
                    }
                    "LIST" => {
                        let length = read_length(file)?;
                        let mut key: Option<String> = None;
                        let mut values = Vec::with_capacity(length as usize);

                        for _ in 0..length {
                            let value_type = read_byte(file)?;
                            if value_type != 0x00 {
                                return Err(anyhow::anyhow!(
                                    "Unexpected value type marker for LIST item: {}",
                                    value_type
                                ));
                            }

                            let current_key = read_string(file)?;
                            let item = read_string(file)?;

                            if key.is_none() {
                                key = Some(current_key);
                            }

                            values.push(RedisObject::String(item));
                        }

                        if let Some(k) = key {
                            db.insert(k, RedisObject::List(values));
                        }
                    }
                    _ => {
                        // Not a data-type token, treat it as expiry key.
                        let expiry_opcode = read_byte(file)?;
                        if expiry_opcode != 0xFC {
                            return Err(anyhow::anyhow!(
                                "Unknown opcode: {}",
                                marker
                            ));
                        }

                        let expiry = read_expiry(file)?;
                        expirations.insert(token, expiry);
                    }
                }
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

fn read_string_with_first_len(file: &mut File, first_len_byte: u8) -> Result<String> {
    let len = read_length_with_first(file, first_len_byte)? as usize;

    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;

    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn read_length(file: &mut File) -> Result<u64> {
    let first = read_byte(file)?;
    read_length_with_first(file, first)
}

fn read_length_with_first(file: &mut File, first: u8) -> Result<u64> {
    if first < 64 {
        return Ok(first as u64);
    }

    if first == 0x80 {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        return Ok(u32::from_be_bytes(buf) as u64);
    }

    Err(anyhow::anyhow!(
        "Unsupported length encoding prefix: 0x{first:02X}"
    ))
}
