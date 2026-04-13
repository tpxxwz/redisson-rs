use fred::types::Value;

pub(crate) mod command_async_executor;
pub(crate) mod command_async_service;
mod command_batch_service;

#[derive(Clone)]
pub enum ListenerMessage {
    Text(String),
    Int(i64),
    Json(Value),
    Binary(Vec<u8>),
}