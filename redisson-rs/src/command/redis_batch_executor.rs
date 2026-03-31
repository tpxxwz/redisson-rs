use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32};

use crate::api::batch_options::BatchOptions;
use crate::client::codec::codec::Codec;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::base_redis_batch_executor::BaseRedisBatchExecutor;
use crate::command::command_batch_service::Entry;
use crate::command::redis_executor::CompletableFuture;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::node_source::NodeSource;
use crate::liveobject::core::redisson_object_builder::{RedissonObjectBuilder, ReferenceType};

// ============================================================
// RedisBatchExecutor<V, R> — 对应 Java org.redisson.command.RedisBatchExecutor<V, R>
// ============================================================

/// 对应 Java org.redisson.command.RedisBatchExecutor<V, R>。
/// 继承自 BaseRedisBatchExecutor，覆写 execute()。
/// IN_MEMORY / IN_MEMORY_ATOMIC 模式下使用此执行器：
/// 将命令加入 batch commands 队列（不走 MULTI/EXEC），最后由 RedisCommonBatchExecutor 统一发送。
///
/// Java 中通过继承 BaseRedisBatchExecutor；Rust 侧用组合实现。
///
/// - `V`：命令解码器返回的中间值类型
/// - `R`：调用方期望的最终结果类型
pub struct RedisBatchExecutor<V, R>
where
    V: 'static,
    R: 'static + Send,
{
    /// 对应 Java 基类 BaseRedisBatchExecutor（组合代替继承）
    pub base: BaseRedisBatchExecutor<V, R>,
}

impl<V, R> RedisBatchExecutor<V, R>
where
    V: 'static,
    R: 'static + Send,
{
    /// 对应 Java RedisBatchExecutor 构造方法（委托给 BaseRedisBatchExecutor）
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        read_only_mode: bool,
        source: NodeSource,
        codec: Option<Box<dyn Codec + Send + Sync>>,
        command: RedisCommand<V>,
        params: Vec<fred::types::Value>,
        main_promise: Arc<CompletableFuture<R>>,
        ignore_redirect: bool,
        connection_manager: Arc<dyn ConnectionManager + Send + Sync>,
        object_builder: Option<RedissonObjectBuilder>,
        commands: Arc<Mutex<Vec<(NodeSource, Entry)>>>,
        options: BatchOptions,
        index: Arc<AtomicU32>,
        executed: Arc<AtomicBool>,
        reference_type: ReferenceType,
        no_retry: bool,
    ) -> Self {
        Self {
            base: BaseRedisBatchExecutor::new(
                read_only_mode,
                source,
                codec,
                command,
                params,
                main_promise,
                ignore_redirect,
                connection_manager,
                object_builder,
                commands,
                options,
                index,
                executed,
                reference_type,
                no_retry,
            ),
        }
    }

    /// 对应 Java RedisBatchExecutor.execute()。
    /// 将命令加入 batch，出现异常时调用 free() 和 handleError()。
    pub fn execute(&self) {
        todo!()
    }
}
