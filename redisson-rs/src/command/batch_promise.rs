use crate::command::redis_executor::CompletableFuture;
use std::marker::PhantomData;
use std::sync::Arc;

// ============================================================
// BatchPromise<T> — 对应 Java org.redisson.command.BatchPromise<T>
// ============================================================

/// 对应 Java org.redisson.command.BatchPromise<T>。
/// 继承自 CompletableFuture<T>，额外持有一个 sentPromise。
///
/// 在 REDIS_READ_ATOMIC / REDIS_WRITE_ATOMIC 模式下，
/// 每条命令对应一个 BatchPromise：
/// - sentPromise 在命令写入 Redis 连接时完成（MULTI 队列写入确认）
/// - 自身（BatchPromise）在 EXEC 响应返回后完成
///
/// CommandBatchService.executeRedisBasedQueue() 用 allOf(sentPromises) 判断
/// 所有命令已入队，再等 EXEC 返回。
///
/// 对应 Java org.redisson.command.BatchPromise<T>。
pub struct BatchPromise<T>
where
    T: 'static + Send,
{
    /// 对应 Java BatchPromise 自身（CompletableFuture<T> 基类）
    pub promise: Arc<CompletableFuture<T>>,
    /// 对应 Java BatchPromise.sentPromise（命令写入 Redis 连接后完成）
    pub sent_promise: Arc<CompletableFuture<()>>,

    _phantom: PhantomData<T>,
}

impl<T> BatchPromise<T>
where
    T: 'static + Send,
{
    /// 对应 Java new BatchPromise()
    pub fn new() -> Self {
        Self {
            promise: Arc::new(CompletableFuture::default()),
            sent_promise: Arc::new(CompletableFuture::default()),
            _phantom: PhantomData,
        }
    }

    /// 对应 Java BatchPromise.getSentPromise()
    pub fn get_sent_promise(&self) -> &Arc<CompletableFuture<()>> {
        &self.sent_promise
    }

    /// 对应 Java BatchPromise.cancel(boolean)（始终返回 false，不可取消）
    pub fn cancel(&self, _may_interrupt: bool) -> bool {
        false
    }
}

impl<T> Default for BatchPromise<T>
where
    T: 'static + Send,
{
    fn default() -> Self {
        Self::new()
    }
}
