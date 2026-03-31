use std::fmt::{Display, Formatter};

// ============================================================
// RedisException — 对应 Java org.redisson.client.RedisException（类）
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisException {
    message: String,
}

impl RedisException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for RedisException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisException {}
