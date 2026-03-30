use fred::types::Value;
use crate::api::rbucket::RBucket;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::client::protocol::redis_commands as commands;
use crate::ext::RedisKey;
use anyhow::Result;
use std::sync::Arc;

// ============================================================
// RedissonBucket — 对应 Java org.redisson.RedissonBucket
// ============================================================

/// Redis String 结构，对应 Java RedissonBucket<String>。
/// CE 为命令执行器类型，CommandAsyncService（normal）或 CommandBatchService（batch）均可。
pub struct RedissonBucket<CE: CommandAsyncExecutor> {
    pub(crate) command_executor: Arc<CE>,
    /// 对应 Java RedissonObject.name
    pub(crate) name: String,
}

impl<CE: CommandAsyncExecutor> RedissonBucket<CE> {
    pub fn new(command_executor: &Arc<CE>, name: impl RedisKey) -> Self {
        Self {
            command_executor: command_executor.clone(),
            name: name.key(),
        }
    }
}

// ============================================================
// RBucket impl
// ============================================================

impl<CE: CommandAsyncExecutor> RBucket for RedissonBucket<CE> {
    /// 对应 Java RBucketAsync.getAsync() — GET key
    async fn get(&self) -> Result<Option<String>> {
        self.command_executor
            .read_async(&self.name, commands::GET, Vec::<Value>::new())
            .await
    }

    /// 对应 Java RBucketAsync.setAsync(value) — SET key value
    async fn set(&self, value: &str) -> Result<()> {
        self.command_executor
            .write_async(&self.name, commands::SET, vec![Value::from(value)])
            .await
    }

    /// 对应 Java RObjectAsync.deleteAsync() — DEL key
    async fn delete(&self) -> Result<bool> {
        let count: i64 = self
            .command_executor
            .write_async(&self.name, commands::DEL, Vec::<Value>::new())
            .await?;
        Ok(count > 0)
    }

    /// 对应 Java RBucketAsync.sizeAsync() — STRLEN key
    async fn size(&self) -> Result<i64> {
        self.command_executor
            .read_async(&self.name, commands::STRLEN, Vec::<Value>::new())
            .await
    }
}
