use fred::types::Value;

pub(crate) mod command_async_executor;
pub(crate) mod command_async_service;
mod command_batch_service;

#[derive(Clone, Debug)]
pub enum ListenerMessage {
    Text(String),
    Int(i64),
    Json(Value),
    Binary(Vec<u8>),
}

impl ListenerMessage {
    pub fn from_value(value: Value) -> Self {
        match value {
            Value::String(s) => Self::Text(s.to_string()),
            Value::Integer(i) => Self::Int(i),
            Value::Bytes(b) => Self::Binary(b.to_vec()),
            other => Self::Json(other),
        }
    }
}