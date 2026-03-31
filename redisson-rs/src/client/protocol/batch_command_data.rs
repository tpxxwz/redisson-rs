use crate::client::codec::codec::Codec;
use crate::client::protocol::command_data::CommandData;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::redis_executor::CompletableFuture;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

// ============================================================
// BatchCommandData<T, R> — 对应 Java org.redisson.client.protocol.BatchCommandData<T, R>
// ============================================================

/// 对应 Java org.redisson.client.protocol.BatchCommandData<T, R>。
/// 在 CommandData 基础上添加批处理顺序索引和重试错误信息。
///
/// Java 中 BatchCommandData 继承 CommandData；Rust 用组合替代继承。
pub struct BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    /// 对应 Java 基类 CommandData 的字段（组合代替继承）
    pub inner: CommandData<T, R>,
    /// 对应 Java BatchCommandData.index（命令在整个 batch 中的顺序，用于最终结果排序）
    pub index: u32,
    /// 对应 Java BatchCommandData.retryError（AtomicReference<RedisException>，重试失败的原因）
    pub retry_error: Mutex<Option<Box<dyn std::error::Error + Send + Sync>>>,

    _phantom: PhantomData<(T, R)>,
}

impl<T, R> BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    /// 对应 Java new BatchCommandData(RedisCommand<T> command, Object[] params, int index)
    pub fn new(command: RedisCommand<T>, params: Vec<fred::types::Value>, index: u32) -> Self {
        let promise = Arc::new(CompletableFuture::default());
        Self {
            inner: CommandData::new(promise, None, command, params),
            index,
            retry_error: Mutex::new(None),
            _phantom: PhantomData,
        }
    }

    /// 对应 Java new BatchCommandData(CompletableFuture<R> promise, Codec codec,
    ///     RedisCommand<T> command, Object[] params, int index)
    pub fn new_with_promise(
        promise: Arc<CompletableFuture<R>>,
        codec: Option<Box<dyn Codec + Send + Sync>>,
        command: RedisCommand<T>,
        params: Vec<fred::types::Value>,
        index: u32,
    ) -> Self {
        Self {
            inner: CommandData::new(promise, codec, command, params),
            index,
            retry_error: Mutex::new(None),
            _phantom: PhantomData,
        }
    }

    /// 对应 Java BatchCommandData.getIndex()
    pub fn get_index(&self) -> u32 {
        self.index
    }

    /// 对应 Java BatchCommandData.tryFailure(Throwable)
    pub fn try_failure(&self, cause: Box<dyn std::error::Error + Send + Sync>) -> bool {
        self.inner.try_failure(cause)
    }

    /// 对应 Java BatchCommandData.isSuccess()
    pub fn is_success(&self) -> bool {
        self.inner.is_success()
    }

    /// 对应 Java BatchCommandData.cause()
    pub fn cause(&self) -> Option<Box<dyn std::error::Error + Send + Sync>> {
        self.inner.cause()
    }

    /// 对应 Java BatchCommandData.clearError()
    /// 清除 retryError，在整个 batch 重试前调用。
    pub fn clear_error(&self) {
        *self.retry_error.lock().unwrap() = None;
    }

    /// 对应 Java BatchCommandData.updateCommand(RedisCommand command)
    /// 用于 loadScripts 阶段将 EVAL 替换为 EVALSHA。
    pub fn update_command(&mut self, _command: RedisCommand<T>) {
        todo!()
    }
}

impl<T, R> PartialEq for BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T, R> Eq for BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
}

impl<T, R> PartialOrd for BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T, R> Ord for BatchCommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    /// 对应 Java BatchCommandData.compareTo()，按 index 升序排列（用于最终结果排序）
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

