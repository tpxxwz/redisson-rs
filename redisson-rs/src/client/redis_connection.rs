use crate::client::redis_client::RedisClient;

// ============================================================
// RedisConnection — 对应 Java org.redisson.client.RedisConnection
// ============================================================

/// 对应 Java org.redisson.client.RedisConnection。
/// 代表与单个 Redis 节点的一条 TCP 连接，负责命令的序列化发送和响应接收。
///
/// Java 中基于 Netty Channel 实现；Rust 侧由 fred Pool 统一管理连接生命周期，
/// 此类作为占位结构对齐抽象层次，待后续接入具体实现。
pub struct RedisConnection {
    /// 对应 Java RedisConnection.redisClient（持有创建此连接的 RedisClient）
    pub redis_client: RedisClient,
}

impl RedisConnection {
    pub fn new(redis_client: RedisClient) -> Self {
        Self { redis_client }
    }

    /// 对应 Java RedisConnection.getRedisClient()
    pub fn get_redis_client(&self) -> &RedisClient {
        &self.redis_client
    }

    /// 对应 Java RedisConnection.forceFastReconnectAsync()
    /// 强制断开并快速重连（用于取消阻塞命令时）。
    pub async fn force_fast_reconnect_async(&self) -> anyhow::Result<()> {
        todo!()
    }
}
