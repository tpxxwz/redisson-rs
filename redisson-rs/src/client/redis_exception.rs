use crate::misc::redis_uri::RedisURI;
use std::fmt::{Display, Formatter};

// ============================================================
// RedisException — 对应 Java org.redisson.client.RedisException
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisException {
    message: String,
}

impl RedisException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisException {}

// ============================================================
// RedisWrongPasswordException
// 对应 Java org.redisson.client.RedisWrongPasswordException
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisWrongPasswordException {
    pub message: String,
}

impl RedisWrongPasswordException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisWrongPasswordException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisWrongPasswordException {}

// ============================================================
// RedisRedirectException
// 对应 Java org.redisson.client.RedisRedirectException
// 携带重定向目标槽号和地址，是 MOVED/ASK 异常的基类。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisRedirectException {
    /// 对应 Java RedisRedirectException.slot
    pub slot: u16,
    /// 对应 Java RedisRedirectException.url
    pub url: RedisURI,
    pub message: String,
}

impl RedisRedirectException {
    pub fn new(slot: u16, url: RedisURI, message: impl Into<String>) -> Self {
        Self { slot, url, message: message.into() }
    }
}

impl Display for RedisRedirectException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisRedirectException {}

// ============================================================
// RedisMovedException
// 对应 Java org.redisson.client.RedisMovedException
// MOVED 重定向：key 已永久迁移到新节点。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisMovedException {
    pub inner: RedisRedirectException,
}

impl RedisMovedException {
    pub fn new(slot: u16, url: RedisURI, message: impl Into<String>) -> Self {
        Self { inner: RedisRedirectException::new(slot, url, message) }
    }
}

impl Display for RedisMovedException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for RedisMovedException {}

// ============================================================
// RedisAskException
// 对应 Java org.redisson.client.RedisAskException
// ASK 重定向：key 正在迁移中，本次请求重定向到新节点（临时）。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisAskException {
    pub inner: RedisRedirectException,
}

impl RedisAskException {
    pub fn new(slot: u16, url: RedisURI, message: impl Into<String>) -> Self {
        Self { inner: RedisRedirectException::new(slot, url, message) }
    }
}

impl Display for RedisAskException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for RedisAskException {}

// ============================================================
// RedisLoadingException
// 对应 Java org.redisson.client.RedisLoadingException
// 节点正在加载 RDB/AOF，暂时无法响应命令。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisLoadingException {
    pub message: String,
}

impl RedisLoadingException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisLoadingException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisLoadingException {}

// ============================================================
// RedisRetryException
// 对应 Java org.redisson.client.RedisRetryException
// 服务端指示本次命令可以重试。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisRetryException {
    pub message: String,
}

impl RedisRetryException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisRetryException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisRetryException {}

// ============================================================
// RedisReadonlyException
// 对应 Java org.redisson.client.RedisReadonlyException
// 写操作打到了只读节点（主从切换中）。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisReadonlyException {
    pub message: String,
}

impl RedisReadonlyException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisReadonlyException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisReadonlyException {}

// ============================================================
// RedisReconnectedException
// 对应 Java org.redisson.client.RedisReconnectedException
// 连接断开且可重发（writeFuture 可取消时）。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisReconnectedException {
    pub message: String,
}

impl RedisReconnectedException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisReconnectedException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisReconnectedException {}

// ============================================================
// RedisTimeoutException
// 对应 Java org.redisson.client.RedisTimeoutException
// 连接获取超时 或 命令写入超时。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisTimeoutException {
    pub message: String,
}

impl RedisTimeoutException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisTimeoutException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisTimeoutException {}

// ============================================================
// RedisResponseTimeoutException
// 对应 Java org.redisson.client.RedisResponseTimeoutException
// Redis 服务端响应超时（命令已发出但未在 responseTimeout 内收到回复）。
// ============================================================

#[derive(Debug, Clone)]
pub struct RedisResponseTimeoutException {
    pub message: String,
}

impl RedisResponseTimeoutException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for RedisResponseTimeoutException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RedisResponseTimeoutException {}

// ============================================================
// WriteRedisConnectionException
// 对应 Java org.redisson.client.WriteRedisConnectionException
// 命令写入连接失败（Netty ChannelFuture 未成功）。
// ============================================================

#[derive(Debug)]
pub struct WriteRedisConnectionException {
    pub message: String,
    pub cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl WriteRedisConnectionException {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), cause: None }
    }

    pub fn with_cause(
        message: impl Into<String>,
        cause: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self { message: message.into(), cause: Some(cause) }
    }
}

impl Display for WriteRedisConnectionException {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WriteRedisConnectionException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}
