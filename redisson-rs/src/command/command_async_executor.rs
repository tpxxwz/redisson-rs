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
