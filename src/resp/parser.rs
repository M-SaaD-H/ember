use std::fmt;

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
    pub fn parse(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        if buf.is_empty() {
            return Err(ParseError::Incomplete);
        }

        if Self::is_inline(buf) {
            return Self::parse_inline(buf);
        }

        match buf[0] as char {
            '+' => Self::parse_simple(buf),
            '-' => Self::parse_simple(buf),
            ':' => Self::parse_integer(buf),
            '$' => Self::parse_bulk_string(buf),
            '*' => Self::parse_array(buf),
            '_' => Self::parse_null(buf),
            '#' => Self::parse_bool(buf),
            _   => Err(ParseError::Invalid(format!("Invalid RESP type: {}", buf[0] as char))),
        }
    }

    // Parses both simple string and simple error
    // "+<value>\r\n" OR "-<error>\r\n"
    // e.g. "+OK\r\n" -> "OK"
    fn parse_simple(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
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
    fn parse_integer(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());
            return match utf8_str {
                Ok(str) => {
                    let int: i64 = str.parse()
                        .map_err(|_| ParseError::Invalid("Invalid integer value.".into()))?;
                    Ok((RespType::Integer(int), len + 1))
                }
                Err(_) => Err(ParseError::Invalid("Integer value is not valid.".into())),
            };
        }
        Err(ParseError::Incomplete)
    }

    // "$<length>\r\n<data>\r\n"
    // e.g. "$5\r\nhello\r\n" -> "hello"
    fn parse_bulk_string(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        let (bulk_str_len, bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
                let bulk_str_len = Self::parse_usize_from_buf(data)?;
                (bulk_str_len, len + 1)
            } else {
                return Err(ParseError::Incomplete);
            };

        let data_start = bytes_consumed;
        let data_end = data_start + bulk_str_len;
        
        if data_end + 2 > buf.len() {
            return Err(ParseError::Incomplete);
        }

        let data = &buf[data_start..data_end];

        if &buf[data_end..data_end + 2] != b"\r\n" {
            return Err(ParseError::Invalid("Missing CRLF after bulk string".into()));
        }

        let s = String::from_utf8(data.to_vec())
            .map_err(|_| ParseError::Invalid("Invalid UTF-8".into()))?;

        Ok((RespType::BulkString(s), data_end + 2))
    }

    // "*<number-of-elements>\r\n<element-1>...<element-n>"
    // e.g. "*2\r\n$5\r\nhello\r\n$5\r\nworld\r\n" -> ["hello", "world"]
    fn parse_array(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        let (arr_len, mut bytes_consumed) =
            if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
                let arr_len = Self::parse_usize_from_buf(data)?;
                (arr_len, len + 1)
            } else {
                return Err(ParseError::Incomplete);
            };

        let mut arr = Vec::with_capacity(arr_len);

        for _ in 0..arr_len {
            let (resptype_el, len) = Self::parse(&buf[bytes_consumed..])?;
            arr.push(resptype_el);
            bytes_consumed += len;
        }

        Ok((RespType::Array(arr), bytes_consumed))
    }

    fn parse_null(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        if let Some((_, len)) = Self::read_until_crlf(&buf[1..]) {
            return Ok((RespType::Null, len + 1));
        }
        Err(ParseError::Incomplete)
    }

    fn parse_bool(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(&buf[1..]) {
            let utf8_str = String::from_utf8(data.to_vec());
            return match utf8_str {
                Ok(str) => match str.as_str() {
                    "t" => Ok((RespType::Boolean(true),  len + 1)),
                    "f" => Ok((RespType::Boolean(false), len + 1)),
                    _   => Err(ParseError::Invalid(
                        "Invalid value for boolean. Only \"t\" or \"f\" is accepted.".into(),
                    )),
                },
                Err(_) => Err(ParseError::Invalid("Invalid value for boolean.".into())),
            };
        }
        Err(ParseError::Incomplete)
    }

    // Reads the block of data (till CRLF ("\r\n"))
    fn read_until_crlf(buf: &[u8]) -> Option<(&[u8], usize)> {
        for i in 0..buf.len().saturating_sub(1) {
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

    // Inline parser. It's legacy but still used.
    // have to add this to support redis-benchmark
    fn parse_inline(buf: &[u8]) -> Result<(RespType, usize), ParseError> {
        if let Some((data, len)) = Self::read_until_crlf(buf) {
            let line = String::from_utf8(data.to_vec())
                .map_err(|_| ParseError::Invalid("Invalid inline command".into()))?;

            let parts: Vec<RespType> = line
                .split_whitespace()
                .map(|s| RespType::BulkString(s.to_string()))
                .collect();

            return Ok((RespType::Array(parts), len));
        }
        Err(ParseError::Incomplete)
    }

    fn is_inline(buf: &[u8]) -> bool {
        match buf[0] as char {
            '+' | '-' | ':' | '$' | '*' | '_' | '#' => false,
            _ => true,
        }
    }
}
