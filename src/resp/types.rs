// RESP - REdis Serialization Protocol
// To communicate with the Redis server, Redis clients use
// a protocol called Redis Serialization Protocol (RESP).

// This is kind of a syntax used to talk to the redis server.

use std::io::Result;
use bytes::Bytes;
use tokio::io::AsyncWriteExt;

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

// Pre-computed static byte slices for common responses
static RESP_OK: &[u8] = b"+Ok\r\n";
static RESP_PONG: &[u8] = b"+Pong\r\n";
static RESP_NIL: &[u8] = b"$3\r\nnil\r\n";
static RESP_QUEUED: &[u8] = b"+QUEUED\r\n";
static RESP_CRLF: &[u8] = b"\r\n";

impl RespType {
    // Write the RESP-encoded representation directly into an AsyncWrite
    // (typically a BufWriter<OwnedWriteHalf>), avoiding intermediate
    // String/Vec allocations entirely.
    pub async fn write_to<W: AsyncWriteExt + Unpin>(&self, w: &mut W) -> Result<()> {
        match self {
            RespType::SimpleString(ss) => {
                // Fast path for the most common replies.
                if ss == "Ok" {
                    return w.write_all(RESP_OK).await;
                }
                if ss == "Pong" {
                    return w.write_all(RESP_PONG).await;
                }
                if ss == "QUEUED" {
                    return w.write_all(RESP_QUEUED).await;
                }
                w.write_all(b"+").await?;
                w.write_all(ss.as_bytes()).await?;
                w.write_all(RESP_CRLF).await
            }
            RespType::SimpleError(se) => {
                w.write_all(b"-").await?;
                w.write_all(se.as_bytes()).await?;
                w.write_all(RESP_CRLF).await
            }
            RespType::Integer(n) => {
                // itoa writes into a small stack buffer - no heap allocation.
                let mut itoa_buf = itoa::Buffer::new();
                let s = itoa_buf.format(*n);
                w.write_all(b":").await?;
                w.write_all(s.as_bytes()).await?;
                w.write_all(RESP_CRLF).await
            }
            RespType::BulkString(bs) => {
                if bs == "nil" {
                    return w.write_all(RESP_NIL).await;
                }
                let mut itoa_buf = itoa::Buffer::new();
                let len_str = itoa_buf.format(bs.len());
                w.write_all(b"$").await?;
                w.write_all(len_str.as_bytes()).await?;
                w.write_all(RESP_CRLF).await?;
                w.write_all(bs.as_bytes()).await?;
                w.write_all(RESP_CRLF).await
            }
            RespType::Array(arr) => {
                let mut itoa_buf = itoa::Buffer::new();
                let len_str = itoa_buf.format(arr.len());
                w.write_all(b"*").await?;
                w.write_all(len_str.as_bytes()).await?;
                w.write_all(RESP_CRLF).await?;
                for item in arr {
                    // Box::pin avoids recursive async issues.
                    Box::pin(item.write_to(w)).await?;
                }
                Ok(())
            }
            RespType::Null => {
                w.write_all(b"_\r\n").await
            }
            RespType::Boolean(b) => {
                if *b {
                    w.write_all(b"#t\r\n").await
                } else {
                    w.write_all(b"#f\r\n").await
                }
            }
        }
    }

    // Kept for tests and non-hot-path usage (e.g. RDB, error formatting).
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Bytes {
        Bytes::from(self.to_string().into_bytes())
    }

    #[allow(dead_code)]
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
