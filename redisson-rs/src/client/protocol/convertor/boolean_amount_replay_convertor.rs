use anyhow::Result;
use fred::types::Value;
use super::Convertor;

// ============================================================
// BooleanAmountReplayConvertor — 对应 Java org.redisson.client.protocol.convertor.BooleanAmountReplayConvertor
// (Long) obj > 0
// ============================================================

pub struct BooleanAmountReplayConvertor;

impl Convertor<bool> for BooleanAmountReplayConvertor {
    fn convert(&self, v: Value) -> Result<bool> {
        match v {
            Value::Boolean(b) => Ok(b),
            Value::Integer(n) => Ok(n > 0),
            _ => Ok(false),
        }
    }
}
