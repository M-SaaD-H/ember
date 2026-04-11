use std::fmt;
use bytes::BytesMut;

use crate::resp::types::RespType;

// Error type for RESP parsing that distinguishes between
#[derive(Debug)]
pub enum ParseError {
    // The buffer doesn't contain a complete RESP message yet.
    // The caller should read more data and retry.
    Incomplete,
    // The data is malformed and cannot be parsed.
    Invalid(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Incomplete => write!(f, "Incomplete data"),
            ParseError::Invalid(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

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
    pub fn parse(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        if buf.is_empty() {
            return Err(ParseError::Incomplete);
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
                Err(ParseError::Invalid(format!("Invalid RESP type: {}", buf[0] as char)))
            }
        }
    }

    // Parses both simple string and simple error
    // "+<value>\r\n" OR "-<error>\r\n"
    // e.g. "+OK\r\n" -> "OK"
    fn parse_simple(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(simple_str) => Ok((RespType::SimpleString(simple_str), len + 1)),
                Err(_) => Err(ParseError::Invalid("Simple string value is not a valid utf8 string.".into())),
            };
        }

        Err(ParseError::Incomplete)
    }
    
    // here sign is optional
    // ":[<+|->]<value>\r\n"
    // e.g. ":123\r\n" -> 123
    fn parse_integer(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(str) => {
                    let int: i32 = str.parse()
                        .map_err(|_| ParseError::Invalid("Invalid integer value.".into()))?;
                    Ok((RespType::Integer(int), len + 1))
                },
                Err(_) => Err(ParseError::Invalid("Integer value is not valid.".into())),
            };
        }

        Err(ParseError::Incomplete)
    }

    // "$<length>\r\n<data>\r\n"
    // e.g. "$5\r\nhello\r\n" -> "hello"
    fn parse_bulk_string(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        let (bulk_str_len, bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
                let bulk_str_len = Self::parse_usize_from_buf(data)?;
                (bulk_str_len, len + 1)
            } else {
                return Err(ParseError::Incomplete);
            };

        // check if the buffer contains complete string based on the length parsed
        let bulk_str_end_idx = bytes_consumed + bulk_str_len;
        // need at least bulk_str_end_idx + 2 bytes for the trailing \r\n
        if bulk_str_end_idx + 2 > buf.len() {
            return Err(ParseError::Incomplete);
        }

        if let Some((data, bytes_consumed)) = Self::read_until_crlf(&buf[bytes_consumed..]) {
            if bytes_consumed != bulk_str_len + 2 { // +2 for "\r\n"
                return Err(ParseError::Invalid("Invalid value for bulk string.".into()));
            }
            let utf8_str = String::from_utf8(data.to_vec());
            return match utf8_str {
                Ok(str) => {
                    Ok((RespType::BulkString(str), bulk_str_end_idx + 2))
                },
                Err(_) => Err(ParseError::Invalid("Invalid value for bulk string.".into())),
            };
        }

        Err(ParseError::Incomplete)
    }

    // "*<number-of-elements>\r\n<element-1>...<element-n>""
    // e.g. "*2\r\n$5\r\nhello\r\n$5\r\nworld\r\n" -> ["hello", "world"]
    fn parse_array(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        let (arr_len, mut bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
                let arr_len = Self::parse_usize_from_buf(data)?;
                (arr_len, len + 1)
            } else {
                return Err(ParseError::Incomplete);
            };

        let mut arr = Vec::new();

        for _ in 0..arr_len {
            let (resptype_el, len) = Self::parse(&BytesMut::from(&buf[bytes_consumed..]))?;

            arr.push(resptype_el);
            bytes_consumed += len;
        }

        Ok((RespType::Array(arr), bytes_consumed))
    }

    fn parse_null(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        if let Some((_, len)) = Self::read_until_crlf(&buf[1..]) {
            return Ok((RespType::Null, len + 1));
        }

        Err(ParseError::Incomplete)
    }

    fn parse_bool(buf: &BytesMut) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());

            return match utf8_str {
                Ok(str) => {
                    match str.as_str() {
                        "t" => Ok((RespType::Boolean(true), len)),
                        "f" => Ok((RespType::Boolean(false), len)),
                        _ => Err(ParseError::Invalid("Invalid value for boolean. Only \"t\" or \"f\" is accepted.".into())),
                    }
                }
                Err(_) => Err(ParseError::Invalid("Invalid value for boolean.".into())),
            };
        }

        Err(ParseError::Incomplete)
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
    fn parse_usize_from_buf(buf: &[u8]) -> Result<usize, ParseError> {
        let utf8_str = String::from_utf8(buf.to_vec())
            .map_err(|_| ParseError::Invalid("Invalid UTF-8 string.".into()))?;
        utf8_str.parse()
            .map_err(|_| ParseError::Invalid("Invalid value for an integer.".into()))
    }
}
