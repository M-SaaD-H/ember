// RESP - REdis Serialization Protocol
// To communicate with the Redis server, Redis clients use
// a protocol called Redis Serialization Protocol (RESP).

// This is kind of a syntax used to talk to the redis server.

use bytes::Bytes;

#[derive(Clone, Debug, PartialEq)]
pub enum RespType {
    SimpleString(String),
    SimpleError(String),
    Integer(i32),
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
            RespType::BulkString(bs) => {
                // '.chars().count()' is used instead of '.len()' because
                // - '.chars().count()' returns the count of the characters in a string.
                // - '.len()' returns the no. of bytes in the string
                // rust strings are UTF-8 encoded.
                // normal strings contains only simple ASCII chars the results for both will be same
                // but if the string contains some emoji or any other character from lang other than
                // english than the no. of bytes and no. of characters will not be the same.
                // Hence to avoid this issue '.chars().count()' is used instead of the conventional '.len()'
                format!("${}\r\n{}\r\n", bs.chars().count(), bs)
            },
            RespType::Array(arr) => {
                let mut arr_str = format!("*{}\r\n", arr.len());

                for a in arr {
                    println!("{}", a.to_string());
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
