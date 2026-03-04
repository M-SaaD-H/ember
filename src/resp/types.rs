// RESP - REdis Serialization Protocol
// To communicate with the Redis server, Redis clients use
// a protocol called Redis Serialization Protocol (RESP).

// This is a kind of a syntax used to talk to the redis server.

use bytes::Bytes;

#[derive(Clone, Debug)]
pub enum RespType {
    SimpleString(String),
    SimpleError(String),
    Integer(i32),
    BulkString(String),
    // Array(Vec<RespType>),
    // Null
}

impl RespType {
    pub fn to_bytes(&self) -> Bytes {
        match self {
            RespType::SimpleString(ss) => Bytes::from_iter(format!("+{}\r\n", ss).into_bytes()),
            RespType::Integer(int) => Bytes::from_iter(format!(":{}\r\n", int).into_bytes()),
            RespType::BulkString(bs) => {
                // '.chars().count()' is used instead of '.len()' because
                // - '.chars().count()' returns the count of the characters in a string.
                // - '.len()' returns the no. of bytes in the string
                // rust strings are UTF-8 encoded.
                // normal strings contains only simple ASCII chars the results for both will be same
                // but if the string contains some emoji or any other character from lang other than
                // english than the no. of bytes and no. of characters will not be the same.
                // Hence to avoid this issue '.chars().count()' is used instead of the conventional '.len()'
                let bulk_str_bytes = format!("${}\r\n{}\r\n", bs.chars().count(), bs);
                Bytes::from_iter(bulk_str_bytes.into_bytes())
            }
            RespType::SimpleError(se) => Bytes::from_iter(format!("-{}\r\n", se).into_bytes()),
        }
    }
}
