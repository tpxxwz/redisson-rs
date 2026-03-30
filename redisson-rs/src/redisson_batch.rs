use crate::api::rbatch::RBatch;
use crate::command::command_async_service::CommandAsyncService;
use crate::command::command_batch_service::CommandBatchService;
use crate::ext::RedisKey;
use crate::redisson_bucket::RedissonBucket;
use anyhow::Result;
use std::sync::Arc;

// ============================================================
// RedissonBatch — 对应 Java org.redisson.RedissonBatch implements RBatch
// ============================================================

/// 批处理客户端，对应 Java RedissonBatch implements RBatch。
///
/// 不暴露锁相关方法——锁涉及 pub/sub 等待和 watchdog 续期，不适合 batch 模式。
/// 只暴露数据结构操作（getBucket / getMap / getAtomicLong 等，待后续补充），
/// 调用 execute() 后所有已入队命令通过 Pipeline 一次性发送。
pub struct RedissonBatch {
    pub(crate) command_executor: Arc<CommandBatchService>,
}

impl RedissonBatch {
    pub fn new(base: Arc<CommandAsyncService>) -> Self {
        Self {
            command_executor: Arc::new(CommandBatchService::new(base)),
        }
    }

    pub fn command_executor(&self) -> &Arc<CommandBatchService> {
        &self.command_executor
    }

    /// 对应 Java RedissonBatch.getBucket(String name)
    pub fn get_bucket<K: RedisKey>(&self, name: K) -> RedissonBucket<CommandBatchService> {
        RedissonBucket::new(&self.command_executor, name)
    }
}

// ============================================================
// RBatch impl
// ============================================================

impl RBatch for RedissonBatch {
    async fn execute(&self) -> Result<()> {
        self.command_executor.execute().await
    }
}
