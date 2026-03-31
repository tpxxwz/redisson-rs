use std::fmt::{Display, Formatter};

// ============================================================
// RedissonShutdownException — 对应 Java org.redisson.RedissonShutdownException
// ============================================================

/// 对应 Java org.redisson.RedissonShutdownException。
/// Redisson 实例已关闭时，尝试执行命令会抛出此异常。
#[derive(Debug, Clone)]
pub struct RedissonShutdownException {
    pub message: String,
}

impl RedissonShutdownException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedissonShutdownException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedissonShutdownException {}
