use crate::client::codec::codec::Codec;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::redis_executor::CompletableFuture;
use std::marker::PhantomData;
use std::sync::Arc;

// ============================================================
// CommandData<T, R> — 对应 Java org.redisson.client.protocol.CommandData<T, R>
// ============================================================

/// 对应 Java org.redisson.client.protocol.CommandData<T, R>。
/// 将一条命令和它的 Promise 绑定在一起，随连接传递到底层发送层。
///
/// - `T`：命令解码器的原始返回类型（RedisCommand<T>）
/// - `R`：调用方期望的最终结果类型（CompletableFuture<R>）
pub struct CommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    /// 对应 Java CommandData.promise
    pub promise: Arc<CompletableFuture<R>>,
    /// 对应 Java CommandData.command
    pub command: RedisCommand<T>,
    /// 对应 Java CommandData.params（Object[]）
    pub params: Vec<fred::types::Value>,
    /// 对应 Java CommandData.codec
    pub codec: Option<Box<dyn Codec + Send + Sync>>,

    _phantom: PhantomData<(T, R)>,
}

impl<T, R> CommandData<T, R>
where
    T: 'static,
    R: 'static + Send,
{
    /// 对应 Java new CommandData(CompletableFuture<R> promise, Codec codec,
    ///     RedisCommand<T> command, Object[] params)
    pub fn new(
        promise: Arc<CompletableFuture<R>>,
        codec: Option<Box<dyn Codec + Send + Sync>>,
        command: RedisCommand<T>,
        params: Vec<fred::types::Value>,
    ) -> Self {
        Self { promise, command, params, codec, _phantom: PhantomData }
    }

    /// 对应 Java CommandData.getCommand()
    pub fn get_command(&self) -> &RedisCommand<T> {
        &self.command
    }

    /// 对应 Java CommandData.getParams()
    pub fn get_params(&self) -> &[fred::types::Value] {
        &self.params
    }

    /// 对应 Java CommandData.getPromise()
    pub fn get_promise(&self) -> &Arc<CompletableFuture<R>> {
        &self.promise
    }

    /// 对应 Java CommandData.getCodec()
    pub fn get_codec(&self) -> Option<&(dyn Codec + Send + Sync)> {
        self.codec.as_deref()
    }

    /// 对应 Java CommandData.cause()
    pub fn cause(&self) -> Option<Box<dyn std::error::Error + Send + Sync>> {
        todo!()
    }

    /// 对应 Java CommandData.isSuccess()
    pub fn is_success(&self) -> bool {
        todo!()
    }

    /// 对应 Java CommandData.tryFailure(Throwable)
    pub fn try_failure(&self, _cause: Box<dyn std::error::Error + Send + Sync>) -> bool {
        todo!()
    }

    /// 对应 Java CommandData.isBlockingCommand()
    pub fn is_blocking_command(&self) -> bool {
        todo!()
    }

    /// 对应 Java CommandData.isExecuted()
    pub fn is_executed(&self) -> bool {
        todo!()
    }
}
