use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::error::Error;
use fred::types::{FromValue, Key, MultipleKeys, MultipleValues, Value};
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor
//
// 【为什么持有方用泛型 Arc<CE: CommandAsyncExecutor> 而不是 Arc<dyn CommandAsyncExecutor>】
//
//   原因一（不得不用泛型）：
//     本 trait 的方法返回 impl Future（原生 async fn），属于非对象安全 trait，
//     Arc<dyn CommandAsyncExecutor> 根本编译不过。
//     若改用 Arc<dyn>，必须把所有方法签名改成 Pin<Box<dyn Future>>，
//     代价过高且偏离 Rust 惯用写法。
//
//   原因二（同一 runtime 并存多种实现）：
//     CommandAsyncService  — 普通命令，redisson.getBucket() 等持有
//     CommandBatchService  — 批量命令，redisson.createBatch() 持有
//     CommandTransactionService — 事务命令
//     不同对象在同一 runtime 里同时持有不同实现，这和 ConnectionManager
//     "启动时选一种、全局唯一"的模式不同，泛型在调用点编译期确定类型更自然。
// ============================================================

pub trait CommandAsyncExecutor: Send + Sync + 'static {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    fn service_manager(&self) -> &Arc<ServiceManager>;

    /// 对应 Java BatchService 类型检查，用于 checkNotBatch()
    fn is_batch(&self) -> bool {
        false
    }

    /// 对应 Java isEvalCacheActive()
    fn is_eval_cache_active(&self) -> bool {
        self.connection_manager().config().use_script_cache
    }

    /// 是否从 replica 读取（仅 cluster + read_from_slave=true 时为 true）
    fn use_replica_for_reads(&self) -> bool {
        false
    }

    fn read_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    fn write_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    fn eval_write_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send + 'static,
        R::Error: Into<Error> + Send;

    fn eval_read_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send + 'static,
        R::Error: Into<Error> + Send;
}
