use anyhow::Result;
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
    pub fn parse(buf: &BytesMut) -> Result<(RespType, usize)> {
        if buf.is_empty() {
            return Err(anyhow::anyhow!("Empty buffer."));
        }

        match buf[0] as char {
            '+' => Self::parse_simple(buf),
            '-' => Self::parse_simple(buf),
            ':' => Self::parse_integer(buf),
            '$' => Self::parse_bulk_string(buf),
            '*' => Self::parse_array(buf),
            '_' => Self::parse_null(buf),
            '#' => Self::parse_bool(buf),
            _ => {
                Err(anyhow::anyhow!(format!("Invalid RESP type: {}", buf[0] as char)))
            }
        }
    }

    // Parses both simple string and simple error
    // "+<value>\r\n" OR "-<error>\r\n"
    // e.g. "+OK\r\n" -> "OK"
    fn parse_simple(buf: &BytesMut) -> Result<(RespType, usize)> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(simple_str) => Ok((RespType::SimpleString(simple_str), len + 1)),
                Err(_) => Err(anyhow::anyhow!("Simple string value is not a valid utf8 string.")),
            };
        }

        Err(anyhow::anyhow!("Invalid value for simple string."))
    }
    
    // here sign is optional
    // ":[<+|->]<value>\r\n"
    // e.g. ":123\r\n" -> 123
    fn parse_integer(buf: &BytesMut) -> Result<(RespType, usize)> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
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
    // e.g. "$5\r\nhello\r\n" -> "hello"
    fn parse_bulk_string(buf: &BytesMut) -> Result<(RespType, usize)> {
        let (bulk_str_len, bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
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

        if let Some((data, bytes_consumed)) = Self::read_until_crlf(&buf[bytes_consumed..]) {
            if bytes_consumed != bulk_str_len + 2 { // +2 for "\r\n"
                return Err(anyhow::anyhow!("Invalid value for bulk string."));
            }
            let utf8_str = String::from_utf8(data.to_vec());
            return match utf8_str {
                Ok(str) => {
                    Ok((RespType::BulkString(str), bulk_str_end_idx + 2))
                },
                Err(_) => Err(anyhow::anyhow!("Invalid value for bulk string.")),
            };
        }

        Err(anyhow::anyhow!("Invalid value for bulk string."))
    }

    // "*<number-of-elements>\r\n<element-1>...<element-n>""
    // e.g. "*2\r\n$5\r\nhello\r\n$5\r\nworld\r\n" -> ["hello", "world"]
    fn parse_array(buf: &BytesMut) -> Result<(RespType, usize)> {
        let (arr_len, mut bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
                let arr_len = Self::parse_usize_from_buf(data)?;
                (arr_len, len + 1)
            } else {
                return Err(anyhow::anyhow!("Invalid value for array."));
            };
        
        // can't think of a way to check if the buffer contains all the elements
        // of the array based on the arr_len

        let mut arr = Vec::new();

        for _ in 0..arr_len {
            let (resptype_el, len) = match Self::parse(&BytesMut::from(&buf[bytes_consumed..])) {
                Ok(resptype) => resptype,
                Err(e) => return Err(anyhow::anyhow!("Invalid value for array element.\n-{}", e)),
            };

            arr.push(resptype_el);
            bytes_consumed += len;
        }

        if bytes_consumed < buf.len() - 1 {
            return Err(anyhow::anyhow!("Invalid value for array."));
        }

        Ok((RespType::Array(arr), bytes_consumed))
    }

    fn parse_null(buf: &BytesMut) -> Result<(RespType, usize)> {
        if let Some((_, len)) = Self::read_until_crlf(&buf[1..]) {
            return Ok((RespType::Null, len + 1));
        }

        Err(anyhow::anyhow!("Invalid value for simple string."))
    }

    fn parse_bool(buf: &BytesMut) -> Result<(RespType, usize)> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(str) => {
                    match str.as_str() {
                        "t" => Ok((RespType::Boolean(true), len)),
                        "f" => Ok((RespType::Boolean(false), len)),
                        _ => Err(anyhow::anyhow!("Invalid value for boolean. Only \"t\" or \"f\" is accepted.")),
                    }
                }
                Err(_) => Err(anyhow::anyhow!("Invalid value for boolean.")),
            };
        }

        Err(anyhow::anyhow!("Invalid value for boolean."))
    }

    // Reads the block of data (till CRLF ("\r\n"))
    fn read_until_crlf(buf: &[u8]) -> Option<(&[u8], usize)> {
        for i in 0..(buf.len() -1) {
            if buf[i] == b'\r' && buf[i + 1] == b'\n' {
                return Some((&buf[0..i], i + 2));
            }
        }

        None
    }

    // Parse usize from bytes
    fn parse_usize_from_buf(buf: &[u8]) -> Result<usize> {
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
