use anyhow::Result;
use fred::types::Value;
use super::Convertor;

// ============================================================
// BooleanNotNullReplayConvertor — 对应 Java org.redisson.client.protocol.convertor.BooleanNotNullReplayConvertor
// obj != null → true
// ============================================================

pub struct BooleanNotNullReplayConvertor;

impl Convertor<bool> for BooleanNotNullReplayConvertor {
    fn convert(&self, v: Value) -> Result<bool> {
        Ok(!matches!(v, Value::Null))
    }
}
