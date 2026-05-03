// RESP - REdis Serialization Protocol
// To communicate with the Redis server, Redis clients use
// a protocol called Redis Serialization Protocol (RESP).

// This is kind of a syntax used to talk to the redis server.

use bytes::Bytes;

#[derive(Clone, Debug, PartialEq)]
pub enum RespType {
    SimpleString(String),
    SimpleError(String),
    Integer(i64),
    BulkString(String),
    Array(Vec<RespType>),
    Null,
    Boolean(bool),
}

impl RespType {
    pub fn to_bytes(&self) -> Bytes {
        Bytes::from_iter(self.to_string().into_bytes())
    }

    pub fn to_string(&self) -> String {
        match self {
            RespType::SimpleString(ss) => format!("+{}\r\n", ss),
            RespType::Integer(int) => format!(":{}\r\n", int),
            RespType::BulkString(bs) => format!("${}\r\n{}\r\n", bs.len(), bs),
            RespType::Array(arr) => {
                let mut arr_str = format!("*{}\r\n", arr.len());

                for a in arr {
                    arr_str.push_str(a.to_string().as_str());
                }

                arr_str
            },
            RespType::Null => format!("_\r\n"),
            RespType::Boolean(b) => format!("#{}\r\n", if *b { "t" } else { "f" }),
            RespType::SimpleError(se) => format!("-{}\r\n", se),
        }
    }
}
