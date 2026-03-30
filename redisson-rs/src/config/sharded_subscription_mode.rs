// ============================================================
// ShardedSubscriptionMode — 对应 Java org.redisson.config.ShardedSubscriptionMode
// ============================================================

/// Sharded Pub/Sub 模式，对应 Java ShardedSubscriptionMode（仅 cluster 模式下生效）。
#[derive(Clone, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShardedSubscriptionMode {
    /// 自动探测：向 Redis 发送 PUBSUB SHARDNUMSUB，成功则启用 spublish（Redis 7.0+）
    #[default]
    Auto,
    /// 强制启用 spublish
    On,
    /// 强制禁用，始终使用 publish
    Off,
}
