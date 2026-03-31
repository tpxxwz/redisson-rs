use bytes::Bytes;
use fred::types::Value;

// ============================================================
// Codec — 对应 Java org.redisson.client.codec.Codec（接口）
// ============================================================

pub trait Codec: Send + Sync {
    fn encode(&self, value: &Value) -> Bytes {
        Bytes::from(format!("{value:?}"))
    }

    fn encode_map_key(&self, value: &Value) -> Bytes {
        self.encode(value)
    }

    fn encode_map_value(&self, value: &Value) -> Bytes {
        self.encode(value)
    }
}

#[derive(Default)]
pub struct StringCodec;

impl Codec for StringCodec {}
