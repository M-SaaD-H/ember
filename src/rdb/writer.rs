use std::{collections::HashMap, fs::File, io::{BufWriter, Write}, time::{Instant, SystemTime, UNIX_EPOCH}};

use anyhow::Result;

use crate::database::core::RedisObject;

pub fn save_rdb(
    file_path: &str,
    db: &HashMap<String, RedisObject>,
    expirations: &HashMap<String, Instant>
) -> Result<()> {
    // firt write in a tmp file, if anything goes wrong then the previous
    // snapshot will not be altered by this.
    let temp_path = format!("{}.tmp", file_path);

    let mut file = BufWriter::new(File::create(&temp_path)?);

    // writing header
    file.write_all(b"REDIS0001")?;

    // writing db
    write_db(&mut file, db, expirations)?;

    file.flush()?;
    std::fs::rename(temp_path, file_path)?;

    Ok(())
}

fn write_db(
    writer: &mut impl Write,
    db: &HashMap<String, RedisObject>,
    expirations: &HashMap<String, Instant>
) -> Result<()> {
    // DB selector (0xFE)
    writer.write_all(&[0xFE])?;
    write_length(writer, 0)?; // DB 0
    
    for (key, value) in db {
        match value {
            RedisObject::String(s) => {
                write_string(writer, "string");
                write_value(writer, key, s)?;
            }
            RedisObject::List(l) => {
                write_string(writer, "list");
                write_length(writer, l.len() as u64);
                for item in l {
                    if let RedisObject::String(s) = item {
                        write_value(writer, key, s)?;
                    }
                }
            }
            _ => {}
        }
   
    }

    for (key, expire) in expirations {
        write_expiry(writer, key, *expire);
    }

    Ok(())
}

fn write_expiry(
    writer: &mut impl Write,
    key: &str,
    expire_instant: Instant,
) -> Result<()> {
    write_string(writer, key);
    
    // Instant - run time only for faster checks
    // SystemTime - for persistence
    
    // Convert Instant → SystemTime
    let now_instant = Instant::now();
    let now_system = SystemTime::now();

    let expire_system = if expire_instant > now_instant {
        now_system + (expire_instant - now_instant)
    } else {
        now_system // already expired
    };

    // Convert to millis since epoch
    let ms = expire_system
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Write opcode + timestamp
    writer.write_all(&[0xFC])?; // ms expiry opcode
    writer.write_all(&ms.to_le_bytes())?;

    Ok(())
}

fn write_value(
    writer: &mut impl Write,
    key: &str,
    value: &str,
) -> std::io::Result<()> {

    writer.write_all(&[0x00])?; // string type

    write_string(writer, key)?;
    write_string(writer, value)?;

    Ok(())
}

fn write_length(writer: &mut impl Write, len: u64) -> std::io::Result<()> {
    if len < 64 {
        writer.write_all(&[len as u8])?;
    } else {
        writer.write_all(&[0x80])?;
        writer.write_all(&(len as u32).to_be_bytes())?;
    }
    Ok(())
}

fn write_string(writer: &mut impl Write, s: &str) -> std::io::Result<()> {
    write_length(writer, s.len() as u64)?;
    writer.write_all(s.as_bytes())?;
    Ok(())
}
