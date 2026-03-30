use super::batch_handle::BatchHandle;
use super::command_async_executor::CommandAsyncExecutor;
use super::command_async_service::CommandAsyncService;
use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::{Result, anyhow};
use fred::error::Error;
use fred::interfaces::ClientLike;
use fred::types::{ClusterHash, CustomCommand, FromValue, Key, MultipleKeys, MultipleValues, Value};
use parking_lot::Mutex;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::oneshot;

// ============================================================
// BatchEntry
// ============================================================

enum BatchEntry {
    Command {
        cmd_name: &'static str,
        key: String,
        args: Vec<Value>,
        is_read: bool,
        tx: oneshot::Sender<Result<Value>>,
    },
    Eval {
        cmd_name: &'static str,
        script: String,
        routing_slot: u16,
        keys: Vec<String>,
        args: Vec<Value>,
        is_read: bool,
        tx: oneshot::Sender<Result<Value>>,
    },
}

// ============================================================
// CommandBatchService — 对应 Java CommandBatchService
// ============================================================

pub struct CommandBatchService {
    pub(crate) base: Arc<CommandAsyncService>,
    queue: Mutex<Vec<BatchEntry>>,
}

impl CommandBatchService {
    pub fn new(base: Arc<CommandAsyncService>) -> Self {
        Self {
            base,
            queue: Mutex::new(Vec::new()),
        }
    }

    /// 对应 Java RBatch.execute()
    pub async fn execute(&self) -> Result<()> {
        let entries: Vec<BatchEntry> = std::mem::take(&mut *self.queue.lock());

        if entries.is_empty() {
            return Ok(());
        }

        let all_reads = entries.iter().all(|e| match e {
            BatchEntry::Command { is_read, .. } => *is_read,
            BatchEntry::Eval { is_read, .. } => *is_read,
        });
        let use_replica = self.base.connection_manager.use_replica_for_reads && all_reads;

        macro_rules! run_pipeline {
            ($pipeline:expr) => {{
                for entry in &entries {
                    match entry {
                        BatchEntry::Command { cmd_name, key, args, .. } => {
                            let slot = ClusterHash::Custom(self.base.connection_manager.calc_slot(key.as_bytes()));
                            let cmd = CustomCommand::new_static(cmd_name, slot, false);
                            let _: Value = $pipeline.custom(cmd, args.clone()).await?;
                        }
                        BatchEntry::Eval { cmd_name, script, routing_slot, keys, args, .. } => {
                            let cmd = CustomCommand::new_static(cmd_name, ClusterHash::Custom(*routing_slot), false);
                            let numkeys = keys.len().to_string();
                            let mut all_args: Vec<Value> = vec![script.clone().into(), numkeys.into()];
                            all_args.extend(keys.iter().cloned().map(|k| k.into()));
                            all_args.extend(args.iter().cloned());
                            let _: Value = $pipeline.custom(cmd, all_args).await?;
                        }
                    }
                }
                $pipeline.try_all::<Value>().await
            }};
        }

        let results = if use_replica {
            run_pipeline!(self.base.pool().replicas().pipeline())
        } else {
            run_pipeline!(self.base.pool().next().pipeline())
        };

        for (entry, result) in entries.into_iter().zip(results.into_iter()) {
            let tx = match entry {
                BatchEntry::Command { tx, .. } => tx,
                BatchEntry::Eval { tx, .. } => tx,
            };
            let _ = tx.send(result.map_err(|e| anyhow!(e)));
        }

        Ok(())
    }
}

// ============================================================
// CommandAsyncExecutor impl — Output<T> = BatchHandle<T>
// ============================================================

impl CommandAsyncExecutor for CommandBatchService {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.base.connection_manager()
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        self.base.service_manager()
    }

    fn is_batch(&self) -> bool {
        true
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
        R::Error: Into<Error> + Send,
    {
        let key = key.into().as_str_lossy().into_owned();
        let args: Vec<Value> = args.into_iter().filter_map(|v| v.try_into().ok()).collect();
        self.async_inner(command.name, key, args, true)
    }

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
        R::Error: Into<Error> + Send,
    {
        let key = key.into().as_str_lossy().into_owned();
        let args: Vec<Value> = args.into_iter().filter_map(|v| v.try_into().ok()).collect();
        self.async_inner(command.name, key, args, false)
    }

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
        R::Error: Into<Error> + Send,
    {
        self.base.eval_write_async(key, command, script, keys, args)
    }

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
        R::Error: Into<Error> + Send,
    {
        self.base.eval_read_async(key, command, script, keys, args)
    }
}

// ============================================================
// CommandBatchService helper
// ============================================================

impl CommandBatchService {
    fn async_inner<T>(
        &self,
        cmd_name: &'static str,
        key: String,
        args: Vec<Value>,
        is_read: bool,
    ) -> BatchHandle<T>
    where
        T: FromValue + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().push(BatchEntry::Command {
            cmd_name,
            key,
            args,
            is_read,
            tx,
        });
        BatchHandle::new(rx)
    }
}
