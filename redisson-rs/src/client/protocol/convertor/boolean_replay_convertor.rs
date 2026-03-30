use anyhow::Result;
use fred::types::Value;
use super::Convertor;

// ============================================================
// BooleanReplayConvertor — 对应 Java org.redisson.client.protocol.convertor.BooleanReplayConvertor
// null→false, 1/"OK"→true, 其他→false
// ============================================================

pub struct BooleanReplayConvertor;

impl Convertor<bool> for BooleanReplayConvertor {
    fn convert(&self, v: Value) -> Result<bool> {
        match v {
            Value::Null => Ok(false),
            Value::Boolean(b) => Ok(b),
            Value::Integer(n) => Ok(n == 1),
            Value::String(ref s) if s.as_bytes() == b"OK" => Ok(true),
            Value::Bytes(ref b) if b.as_ref() == b"OK" => Ok(true),
            _ => Ok(false),
        }
    }
}
