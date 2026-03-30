use anyhow::Result;

// ============================================================
// RBatch — 对应 Java org.redisson.api.RBatch
// ============================================================

/// 批处理接口，对应 Java RBatch。
/// 与 RedissonClient 完全独立——RBatch 暴露的是数据结构的 Async 变体
/// （getBucket / getMap / getAtomicLong 等），不包含锁。
/// execute() 触发 Pipeline 一次性发送所有已入队命令。
pub trait RBatch {
    /// 对应 Java RBatch.execute()
    fn execute(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}
