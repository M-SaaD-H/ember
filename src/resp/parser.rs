use anyhow::{Error, Result};
use bytes::BytesMut;

use crate::resp::types::RespType;

// Parser parses the given bytes buffer into the respective types and
// return the RespType and no. of bytes read.
// First byte is the identifier of the type of data followed by the
// data itself.
// Multiple data points are written in a single buffer separated by CRLF ("\r\n")

// example identifiers -
// ':' -> Integers
// '+' -> Simple String
// '$' -> Bulk String etc.
// more identifiers are on <https://redis.io/docs/latest/develop/reference/protocol-spec/#resp-protocol-description>

// 2 types of errors can occur here
// - Invalid RespType
// - parsing fails due to encoding issues

pub struct Parser;

impl Parser {
    pub fn parse(buf: BytesMut) -> Result<(RespType, usize)> {
        if buf.is_empty() {
            return Err(Error::msg("Empty buffer."));
        }

        match buf[0] as char {
            '+' => Self::parse_simple(buf),
            '-' => Self::parse_simple(buf),
            ':' => Self::parse_integer(buf),
            '$' => Self::parse_bulk_string(buf),
            // '*' => Self::parse_array(buf),
            // '_' => Self::parse_null(buf),
            _ => {
                Err(anyhow::anyhow!(format!("Invalid RESP type: {}", buf[0] as char)))
            }
        }
    }

    // Parses both simple string and simple error
    // "+<value>\r\n" OR "-<error>\r\n"
    // e.g. "+OK\r\n" -> OK
    fn parse_simple(buf: BytesMut) -> Result<(RespType, usize)> {
        if let Some((data, len)) = Self::read_block(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(simple_str) => return Ok((RespType::SimpleString(simple_str), len + 1)),
                Err(_) => Err(anyhow::anyhow!("Simple string value is not a valid utf8 string.")),
            };
        }

        Err(anyhow::anyhow!("Invalid value for simple string."))
    }
    
    // here sign is optional
    // ":[<+|->]<value>\r\n"
    // e.g. ":123\r\n" -> 123
    fn parse_integer(buf: BytesMut) -> Result<(RespType, usize)> {
        if let Some((data, len)) = Self::read_block(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(str) => {
                    let int: i32 = str.parse().expect("Invalid integer value.");
                    Ok((RespType::Integer(int), len + 1))
                },
                Err(_) => Err(anyhow::anyhow!("Integer value is not valid.")),
            };
        }

        Err(anyhow::anyhow!("Invalid value for integer."))
    }

    // "$<length>\r\n<data>\r\n"
    // e.g. "$5\r\nhello\r\n"
    fn parse_bulk_string(buf: BytesMut) -> Result<(RespType, usize)> {
        let (bulk_str_len, bytes_consumed) =
            if let Some((data, len)) = Self::read_block(&buf[1..]) {
                let bulk_str_len = Self::parse_usize_from_buf(data)?;
                (bulk_str_len, len + 1)
            } else {
                return Err(anyhow::anyhow!("Invalid value for bulk string."));
            };

        // check if the buffer contains complete string based on the length parsed
        let bulk_str_end_idx = bytes_consumed + bulk_str_len;
        if bulk_str_end_idx >= buf.len() {
            return Err(anyhow::anyhow!("Invalid value for bulk string"));
        }

        let bulk_str = String::from_utf8(
            buf[bytes_consumed..bulk_str_end_idx].to_vec()
        );

        match bulk_str {
            Ok(str) => Ok((RespType::BulkString(str), bulk_str_end_idx + 2)),
            Err(_) => Err(anyhow::anyhow!("Invalid value for bulk string.")),
        }
    }

    // fn parse_array(_buf: BytesMut) -> Result<(RespType, usize)> {
    //     unimplemented!()
    // }

    // fn parse_null(_buf: BytesMut) -> Result<(RespType, usize)> {
    //     unimplemented!()
    // }

    // Reads the block of data (till CRLF ("\r\n"))
    fn read_block(buf: &[u8]) -> Option<(&[u8], usize)> {
        for i in 0..(buf.len() -1) {
            if buf[i] == b'\r' && buf[i + 1] == b'\n' {
                return Some((&buf[0..i], i + 2));
            }
        }

        None
    }

    // Parse usize from bytes
    fn parse_usize_from_buf(buf: &[u8]) -> Result<usize, Error> {
        let utf8_str = String::from_utf8(buf.to_vec());
        let parsed_int = match utf8_str {
            Ok(s) => {
                let int: usize = s.parse().expect("Invalid value for an integer.");
                Ok(int)
            },
            Err(_) => Err(anyhow::anyhow!("Invalid UTF-8 string.")),
        };

        parsed_int
    }
}
