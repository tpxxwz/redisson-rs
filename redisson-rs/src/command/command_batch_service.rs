// use super::command_async_executor::CommandAsyncExecutor;
// use super::command_async_service::CommandAsyncService;
// use crate::connection::connection_manager::ConnectionManager;
// use anyhow::{Result, anyhow};
// use fred::error::Error;
// use fred::interfaces::ClientLike;
// use fred::types::{ClusterHash, CustomCommand, FromValue, Key, MultipleKeys, MultipleValues, Value};
// use parking_lot::Mutex as PLMutex;
// use std::collections::HashMap;
// use std::future::Future;
// use std::sync::atomic::{AtomicBool, AtomicU32};
// use std::sync::{Arc, Mutex};
// use tokio::sync::oneshot;
//
// // ============================================================
// // ConnectionEntry — 对应 Java CommandBatchService.ConnectionEntry（内部静态类）
// // ============================================================
//
// /// 对应 Java CommandBatchService.ConnectionEntry。
// /// 在 REDIS_READ_ATOMIC / REDIS_WRITE_ATOMIC 执行模式下，
// /// 追踪每个节点的连接获取状态，用于 MULTI 命令注入和超时取消。
// pub struct ConnectionEntry {
//     /// 对应 Java ConnectionEntry.firstCommand
//     /// true 表示尚未向该连接注入 MULTI，下一条命令需先发 MULTI
//     pub first_command: bool,
//     /// 对应 Java ConnectionEntry.connectionFuture（CompletableFuture<RedisConnection>）
//     /// 持有正在获取或已获取到的连接 Future
//     pub connection_future: Arc<CompletableFuture<crate::client::redis_connection::RedisConnection>>,
//     /// 对应 Java ConnectionEntry.cancelCallback（Runnable）
//     /// 超时时调用以取消连接获取
//     pub cancel_callback: Option<Box<dyn FnOnce() + Send + Sync>>,
// }
//
// impl ConnectionEntry {
//     /// 对应 Java new ConnectionEntry(CompletableFuture<RedisConnection> connectionFuture)
//     pub fn new(
//         connection_future: Arc<CompletableFuture<crate::client::redis_connection::RedisConnection>>,
//     ) -> Self {
//         Self {
//             first_command: true,
//             connection_future,
//             cancel_callback: None,
//         }
//     }
//
//     /// 对应 Java ConnectionEntry.isFirstCommand()
//     pub fn is_first_command(&self) -> bool {
//         self.first_command
//     }
//
//     /// 对应 Java ConnectionEntry.setFirstCommand(boolean)
//     pub fn set_first_command(&mut self, first_command: bool) {
//         self.first_command = first_command;
//     }
//
//     /// 对应 Java ConnectionEntry.getConnectionFuture()
//     pub fn get_connection_future(
//         &self,
//     ) -> &Arc<CompletableFuture<crate::client::redis_connection::RedisConnection>> {
//         &self.connection_future
//     }
//
//     /// 对应 Java ConnectionEntry.getCancelCallback()
//     pub fn get_cancel_callback(&self) -> Option<&(dyn Fn() + Send + Sync)> {
//         todo!()
//     }
//
//     /// 对应 Java ConnectionEntry.setCancelCallback(Runnable)
//     pub fn set_cancel_callback(&mut self, callback: Box<dyn FnOnce() + Send + Sync>) {
//         self.cancel_callback = Some(callback);
//     }
// }
//
// // ============================================================
// // Entry — 对应 Java CommandBatchService.Entry（内部静态类）
// // ============================================================
//
// /// 对应 Java CommandBatchService.Entry。
// /// 代表发往某个节点的一组命令，内含命令队列、EVAL 子集和读写模式标记。
// ///
// /// 在 batch 执行时，`commands` 按 index 排序后统一发往对应节点；
// /// `eval_commands` 是 EVAL 命令子集，在 `loadScripts` 阶段用于脚本预加载。
// pub struct Entry {
//     /// 对应 Java Entry.evalCommands（LinkedList<BatchCommandData>）
//     /// 仅包含 EVAL 命令，用于 loadScripts 阶段替换为 EVALSHA
//     pub eval_commands: Vec<Box<dyn std::any::Any + Send + Sync>>,
//     /// 对应 Java Entry.commands（ConcurrentLinkedDeque<BatchCommandData>）
//     /// 该节点上的全部命令（按加入顺序，执行前 sortCommands 排序）
//     ///
//     /// # 类型说明
//     /// Java 中 BatchCommandData<?, ?> 使用了通配符泛型，Rust 无对应机制。
//     /// 此处用 `Box<dyn Any>` 作为占位，实际实现时需根据具体泛型策略调整。
//     pub commands: std::collections::VecDeque<Box<dyn std::any::Any + Send + Sync>>,
//     /// 对应 Java Entry.readOnlyMode（volatile boolean）
//     /// 该节点上是否全部为只读命令，用于决定从哪个连接池借连接
//     pub read_only_mode: bool,
// }
//
// impl Entry {
//     pub fn new() -> Self {
//         Self {
//             eval_commands: Vec::new(),
//             commands: std::collections::VecDeque::new(),
//             read_only_mode: true,
//         }
//     }
//
//     /// 对应 Java Entry.addCommand(BatchCommandData command)
//     /// 若是 EVAL_OBJECT 命令则同时加入 eval_commands
//     pub fn add_command(&mut self, command: Box<dyn std::any::Any + Send + Sync>) {
//         // TODO: 若 command 是 EVAL_OBJECT，同时加入 eval_commands
//         self.commands.push_back(command);
//     }
//
//     /// 对应 Java Entry.addFirstCommand(BatchCommandData command)
//     /// 在队列头部插入（用于注入 MULTI 命令）
//     pub fn add_first_command(&mut self, command: Box<dyn std::any::Any + Send + Sync>) {
//         self.commands.push_front(command);
//     }
//
//     /// 对应 Java Entry.add(BatchCommandData command)
//     pub fn add(&mut self, command: Box<dyn std::any::Any + Send + Sync>) {
//         self.commands.push_back(command);
//     }
//
//     /// 对应 Java Entry.sortCommands()
//     /// 按 BatchCommandData.index 升序排列，保证多节点场景下结果顺序一致
//     pub fn sort_commands(&mut self) {
//         todo!()
//     }
//
//     /// 对应 Java Entry.setReadOnlyMode(boolean)
//     pub fn set_read_only_mode(&mut self, read_only_mode: bool) {
//         self.read_only_mode = read_only_mode;
//     }
//
//     /// 对应 Java Entry.isReadOnlyMode()
//     pub fn is_read_only_mode(&self) -> bool {
//         self.read_only_mode
//     }
//
//     /// 对应 Java Entry.clearErrors()
//     /// 清除所有命令的 retryError，在整个 batch 重试前调用
//     pub fn clear_errors(&self) {
//         todo!()
//     }
// }
//
// impl Default for Entry {
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
// // ============================================================
// // BatchEntry（内部队列项，用于现有简化实现）
// // ============================================================
//
// enum BatchEntry {
//     Command {
//         cmd_name: &'static str,
//         key: String,
//         args: Vec<Value>,
//         is_read: bool,
//         tx: oneshot::Sender<Result<Value>>,
//     },
//     Eval {
//         cmd_name: &'static str,
//         script: String,
//         routing_slot: u16,
//         keys: Vec<String>,
//         args: Vec<Value>,
//         is_read: bool,
//         tx: oneshot::Sender<Result<Value>>,
//     },
// }
//
// // ============================================================
// // CommandBatchService — 对应 Java CommandBatchService
// // ============================================================
//
// /// 对应 Java org.redisson.command.CommandBatchService。
// /// 继承自 CommandAsyncService，实现 BatchService 接口。
// ///
// /// 命令先缓冲到内存，execute() 时按执行模式分发：
// /// - IN_MEMORY / IN_MEMORY_ATOMIC：聚合后批量发送（pipeline 或 MULTI/EXEC）
// /// - REDIS_WRITE_ATOMIC / REDIS_READ_ATOMIC：逐条入队到 Redis，最后 EXEC
// pub struct CommandBatchService {
//     /// 对应 Java CommandBatchService 基类 CommandAsyncService 的引用
//     pub(crate) base: Arc<CommandAsyncService>,
//
//     // ── Java 原始字段 ──
//
//     /// 对应 Java CommandBatchService.index（AtomicInteger，命令全局序号）
//     pub index: Arc<AtomicU32>,
//     /// 对应 Java CommandBatchService.commands（ConcurrentMap<NodeSource, Entry>）
//     /// 按 NodeSource 分组的命令 Map
//     ///
//     /// # 注意
//     /// Java 用 NodeSource 作为 Map key（依赖 equals/hashCode）。
//     /// Rust 侧暂用 Vec 代替 HashMap，待 NodeSource 实现 Hash + Eq 后替换。
//     pub commands: Mutex<Vec<(NodeSource, Entry)>>,
//     /// 对应 Java CommandBatchService.aggregatedCommands（Map<MasterSlaveEntry, Entry>）
//     /// resolveCommands 后按 MasterSlaveEntry 聚合的命令（executeRedisBasedQueue 使用）
//     pub aggregated_commands: Mutex<Vec<(Arc<MasterSlaveEntry>, Entry)>>,
//     /// 对应 Java CommandBatchService.connections（ConcurrentMap<MasterSlaveEntry, ConnectionEntry>）
//     /// REDIS_*_ATOMIC 模式下各节点的连接缓存
//     pub connections: Mutex<Vec<(Arc<MasterSlaveEntry>, ConnectionEntry)>>,
//     /// 对应 Java CommandBatchService.options
//     pub options: BatchOptions,
//     /// 对应 Java CommandBatchService.nestedServices
//     /// 嵌套的 CommandBatchService（用于 RBatch 内嵌 RBatch 场景）
//     pub nested_services: Mutex<Vec<(Arc<CompletableFuture<()>>, Vec<Arc<CommandBatchService>>)>>,
//     /// 对应 Java CommandBatchService.executed（AtomicBoolean，防止重复执行）
//     pub executed: Arc<AtomicBool>,
//     /// 对应 Java CommandBatchService.retryDelay
//     pub retry_delay: Arc<dyn DelayStrategy + Send + Sync>,
//     /// 对应 Java CommandBatchService.retryAttempts
//     pub retry_attempts: u32,
//     /// 对应 Java CommandBatchService.referenceType
//     pub reference_type: ReferenceType,
//
//     // ── 当前简化实现的队列（IN_MEMORY 模式 pipeline 快路径）──
//     queue: PLMutex<Vec<BatchEntry>>,
// }
//
// impl CommandBatchService {
//     /// 对应 Java new CommandBatchService(CommandAsyncExecutor executor)
//     pub fn new(base: Arc<CommandAsyncService>) -> Self {
//         let retry_delay = base.connection_manager.config().retry_delay.clone();
//         let retry_attempts = base.connection_manager.config().retry_attempts;
//         Self {
//             base,
//             index: Arc::new(AtomicU32::new(0)),
//             commands: Mutex::new(Vec::new()),
//             aggregated_commands: Mutex::new(Vec::new()),
//             connections: Mutex::new(Vec::new()),
//             options: BatchOptions::defaults(),
//             nested_services: Mutex::new(Vec::new()),
//             executed: Arc::new(AtomicBool::new(false)),
//             retry_delay: Arc::new(retry_delay),
//             retry_attempts,
//             reference_type: ReferenceType::Default,
//             queue: PLMutex::new(Vec::new()),
//         }
//     }
//
//     /// 对应 Java new CommandBatchService(CommandAsyncExecutor executor, BatchOptions options)
//     pub fn new_with_options(base: Arc<CommandAsyncService>, options: BatchOptions) -> Self {
//         let retry_delay = base.connection_manager.config().retry_delay.clone();
//         let retry_attempts = if options.retry_attempts >= 0 {
//             options.retry_attempts as u32
//         } else {
//             base.connection_manager.config().retry_attempts
//         };
//         let retry_delay_arc: Arc<dyn DelayStrategy + Send + Sync> = if let Some(d) = options.retry_delay.clone() {
//             d
//         } else {
//             Arc::new(retry_delay)
//         };
//         Self {
//             base,
//             index: Arc::new(AtomicU32::new(0)),
//             commands: Mutex::new(Vec::new()),
//             aggregated_commands: Mutex::new(Vec::new()),
//             connections: Mutex::new(Vec::new()),
//             options,
//             nested_services: Mutex::new(Vec::new()),
//             executed: Arc::new(AtomicBool::new(false)),
//             retry_delay: retry_delay_arc,
//             retry_attempts,
//             reference_type: ReferenceType::Default,
//             queue: PLMutex::new(Vec::new()),
//         }
//     }
//
//     /// 对应 Java CommandBatchService.getOptions()
//     pub fn get_options(&self) -> &BatchOptions {
//         &self.options
//     }
//
//     /// 对应 Java CommandBatchService.isExecuted()
//     pub fn is_executed(&self) -> bool {
//         self.executed.load(std::sync::atomic::Ordering::Relaxed)
//     }
//
//     /// 对应 Java CommandBatchService.add(CompletableFuture, List<CommandBatchService>)
//     /// 注册嵌套服务（用于 RBatch 内嵌 RBatch 场景）
//     pub fn add_nested(
//         &self,
//         future: Arc<CompletableFuture<()>>,
//         services: Vec<Arc<CommandBatchService>>,
//     ) {
//         self.nested_services.lock().unwrap().push((future, services));
//     }
//
//     /// 对应 Java CommandBatchService.discard()
//     pub fn discard(&self) {
//         todo!()
//     }
//
//     /// 对应 Java CommandBatchService.discardAsync()
//     pub async fn discard_async(&self) -> Result<()> {
//         todo!()
//     }
//
//     /// 对应 Java CommandBatchService.execute()（同步，内部调用 executeAsync）
//     pub async fn execute_sync(&self) -> Result<BatchResult<fred::types::Value>> {
//         todo!()
//     }
//
//     /// 对应 Java CommandBatchService.executeAsyncVoid()
//     pub async fn execute_async_void(&self) -> Result<()> {
//         todo!()
//     }
//
//     /// 对应 Java CommandBatchService.executeAsync()
//     /// 主执行入口：根据执行模式分发到对应路径。
//     pub async fn execute_async(&self) -> Result<BatchResult<fred::types::Value>> {
//         todo!()
//     }
//
//     /// 对应 Java CommandBatchService.isRedisBasedQueue()
//     pub fn is_redis_based_queue(&self) -> bool {
//         self.options.is_redis_based_queue()
//     }
//
//     /// 对应 Java CommandBatchService.isEvalCacheActive()（始终返回 false）
//     pub fn is_eval_cache_active(&self) -> bool {
//         false
//     }
//
//     // ─── 当前简化实现（IN_MEMORY pipeline 快路径）──────────────────
//
//     /// 对应 Java RBatch.execute()（当前简化实现）
//     pub async fn execute(&self) -> Result<()> {
//         let entries: Vec<BatchEntry> = std::mem::take(&mut *self.queue.lock());
//
//         if entries.is_empty() {
//             return Ok(());
//         }
//
//         let all_reads = entries.iter().all(|e| match e {
//             BatchEntry::Command { is_read, .. } => *is_read,
//             BatchEntry::Eval { is_read, .. } => *is_read,
//         });
//         let use_replica = self.base.connection_manager.use_replica_for_reads() && all_reads;
//
//         macro_rules! run_pipeline {
//             ($pipeline:expr) => {{
//                 for entry in &entries {
//                     match entry {
//                         BatchEntry::Command { cmd_name, key, args, .. } => {
//                             let slot = ClusterHash::Custom(
//                                 self.base.connection_manager.calc_slot(key.as_bytes()),
//                             );
//                             let cmd = CustomCommand::new_static(cmd_name, slot, false);
//                             let _: Value = $pipeline.custom(cmd, args.clone()).await?;
//                         }
//                         BatchEntry::Eval {
//                             cmd_name,
//                             script,
//                             routing_slot,
//                             keys,
//                             args,
//                             ..
//                         } => {
//                             let cmd = CustomCommand::new_static(
//                                 cmd_name,
//                                 ClusterHash::Custom(*routing_slot),
//                                 false,
//                             );
//                             let numkeys = keys.len().to_string();
//                             let mut all_args: Vec<Value> =
//                                 vec![script.clone().into(), numkeys.into()];
//                             all_args.extend(keys.iter().cloned().map(|k| k.into()));
//                             all_args.extend(args.iter().cloned());
//                             let _: Value = $pipeline.custom(cmd, all_args).await?;
//                         }
//                     }
//                 }
//                 $pipeline.try_all::<Value>().await
//             }};
//         }
//
//         let results = if use_replica {
//             run_pipeline!(self.base.pool().replicas().pipeline())
//         } else {
//             run_pipeline!(self.base.pool().next().pipeline())
//         };
//
//         for (entry, result) in entries.into_iter().zip(results.into_iter()) {
//             let tx = match entry {
//                 BatchEntry::Command { tx, .. } => tx,
//                 BatchEntry::Eval { tx, .. } => tx,
//             };
//             let _ = tx.send(result.map_err(|e| anyhow!(e)));
//         }
//
//         Ok(())
//     }
// }
//
// impl BatchService for CommandBatchService {}
//
// // ============================================================
// // CommandAsyncExecutor impl
// // ============================================================
//
// impl CommandAsyncExecutor for CommandBatchService {
//     fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
//         self.base.connection_manager()
//     }
//
//     fn service_manager(&self) -> &Arc<ServiceManager> {
//         self.base.service_manager()
//     }
//
//     fn is_batch(&self) -> bool {
//         true
//     }
//
//     fn read_async<T, K, R>(
//         &self,
//         key: K,
//         command: RedisCommand<T>,
//         args: Vec<R>,
//     ) -> impl Future<Output = Result<T>> + Send + 'static
//     where
//         T: FromValue + Send + 'static,
//         K: Into<Key> + Send,
//         R: TryInto<Value> + Send + 'static,
//         R::Error: Into<Error> + Send,
//     {
//         let key = key.into().as_str_lossy().into_owned();
//         let args: Vec<Value> = args.into_iter().filter_map(|v| v.try_into().ok()).collect();
//         self.async_inner(command.name, key, args, true)
//     }
//
//     fn write_async<T, K, R>(
//         &self,
//         key: K,
//         command: RedisCommand<T>,
//         args: Vec<R>,
//     ) -> impl Future<Output = Result<T>> + Send + 'static
//     where
//         T: FromValue + Send + 'static,
//         K: Into<Key> + Send,
//         R: TryInto<Value> + Send + 'static,
//         R::Error: Into<Error> + Send,
//     {
//         let key = key.into().as_str_lossy().into_owned();
//         let args: Vec<Value> = args.into_iter().filter_map(|v| v.try_into().ok()).collect();
//         self.async_inner(command.name, key, args, false)
//     }
//
//     fn eval_write_async<T, K, MK, R>(
//         &self,
//         key: K,
//         command: RedisCommand<T>,
//         script: &str,
//         keys: MK,
//         args: R,
//     ) -> impl Future<Output = Result<T>> + Send + 'static
//     where
//         T: FromValue + Send + 'static,
//         K: Into<Key> + Send,
//         MK: Into<MultipleKeys> + Send,
//         R: TryInto<MultipleValues> + Send + 'static,
//         R::Error: Into<Error> + Send,
//     {
//         self.base.eval_write_async(key, command, script, keys, args)
//     }
//
//     fn eval_read_async<T, K, MK, R>(
//         &self,
//         key: K,
//         command: RedisCommand<T>,
//         script: &str,
//         keys: MK,
//         args: R,
//     ) -> impl Future<Output = Result<T>> + Send + 'static
//     where
//         T: FromValue + Send + 'static,
//         K: Into<Key> + Send,
//         MK: Into<MultipleKeys> + Send,
//         R: TryInto<MultipleValues> + Send + 'static,
//         R::Error: Into<Error> + Send,
//     {
//         self.base.eval_read_async(key, command, script, keys, args)
//     }
// }
//
// // ============================================================
// // CommandBatchService helper（简化实现快路径）
// // ============================================================
//
// impl CommandBatchService {
//     fn async_inner<T>(
//         &self,
//         cmd_name: &'static str,
//         key: String,
//         args: Vec<Value>,
//         is_read: bool,
//     ) -> BatchHandle<T>
//     where
//         T: FromValue + Send + 'static,
//     {
//         let (tx, rx) = oneshot::channel();
//         self.queue.lock().push(BatchEntry::Command {
//             cmd_name,
//             key,
//             args,
//             is_read,
//             tx,
//         });
//         BatchHandle::new(rx)
//     }
// }

use crate::api::batch_options::{BatchOptions, ExecutionMode};
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::command_async_service::{CommandAsyncInner, CommandAsyncServiceLike};
use crate::connection::fred_connection_manager::FredConnectionManager;
use fred::clients::{Client, Pipeline};
use fred::interfaces::ClientLike;
use fred::types::{ClusterHash, CustomCommand, Value};
use fred::util::redis_keyslot;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

// ============================================================
// BatchEntry — 内存队列中的一条待执行命令
// ============================================================

/// IN_MEMORY / IN_MEMORY_ATOMIC 模式下的队列条目。
/// cmd   : 待执行的命令
/// tx    : 执行完成后把结果回填给调用方的 oneshot 发送端
struct BatchEntry {
    cmd: RedisCommand,
    tx:  oneshot::Sender<anyhow::Result<Value>>,
}

// ============================================================
// CommandBatchService
// ============================================================

/// 对应 Java org.redisson.command.CommandBatchService。
/// 持有独立的 CommandAsyncInner（继承自 CommandAsyncService），
/// 以及本次 batch 专属的 BatchOptions（含执行模式、同步配置等）。
pub struct CommandBatchService {
    pub(crate) inner:   CommandAsyncInner,
    pub(crate) options: BatchOptions,

    // ── IN_MEMORY / IN_MEMORY_ATOMIC 字段 ──────────────────
    /// 对应 Java CommandBatchService.commands（NodeSource → Entry 的 Map，此处简化为线性队列）。
    queue: Mutex<Vec<BatchEntry>>,

    // ── REDIS_READ/WRITE_ATOMIC 字段 ───────────────────────
    /// 对应 Java RedisQueuedBatchExecutor 里持有的 sticky connection。
    /// 用 tokio::sync::Mutex 以便在持锁期间跨 await，保证 MULTI 先于所有用户命令入队。
    queued_pipeline: tokio::sync::Mutex<Option<Pipeline<Client>>>,
    /// 与 queued_pipeline 中命令顺序一致（不含 MULTI/EXEC）的 oneshot 发送端列表。
    queued_txs: tokio::sync::Mutex<Vec<oneshot::Sender<anyhow::Result<Value>>>>,
    /// MULTI / EXEC 在集群中路由到的固定 slot。
    /// 由 pool.next().id() 计算，确保整个 batch 始终路由到同一节点，
    /// 对应 Java cluster_hash_legacy_command 里用 client.id() 计算 hash_slot 的思路。
    queued_slot: u16,
}

impl CommandBatchService {
    /// 对应 Java new CommandBatchService(ConnectionManager connectionManager, BatchOptions options)。
    ///
    /// retry_attempts 优先取 options，-1 时置 None（交给 fred 全局配置）。
    /// response_timeout 优先取 options，未设置时置 None（交给 fred 全局配置）。
    /// retryDelay 暂不处理（BatchOptions 尚未携带该字段）。
    pub fn new(connection_manager: Arc<FredConnectionManager>, options: BatchOptions) -> Self {
        // 对应 Java: if (options.getRetryAttempts() >= 0) { ... } else { use config }
        let retry_attempts = if options.get_retry_attempts() >= 0 {
            Some(options.get_retry_attempts() as u32)
        } else {
            None // fred 使用全局配置
        };
        let response_timeout = options.get_response_timeout();

        // 用 pool 首个 client 的 ID 计算固定 slot，对应 cluster_hash_legacy_command 的思路
        let queued_slot = redis_keyslot(connection_manager.pool.next().id().as_bytes());

        Self {
            inner: CommandAsyncInner::new_all_params(
                connection_manager,
                retry_attempts,
                response_timeout,
                false,
            ),
            options,
            queue:            Mutex::new(Vec::new()),
            queued_pipeline:  tokio::sync::Mutex::new(None),
            queued_txs:       tokio::sync::Mutex::new(Vec::new()),
            queued_slot,
        }
    }

    /// 使用默认 BatchOptions 构造。
    pub fn new_default(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self::new(connection_manager, BatchOptions::defaults())
    }

    /// 对应 Java CommandBatchService.getOptions()
    pub fn get_options(&self) -> &BatchOptions {
        &self.options
    }

    /// 对应 Java CommandBatchService.isRedisBasedQueue()
    pub fn is_redis_based_queue(&self) -> bool {
        self.options.is_redis_based_queue()
    }

    // --------------------------------------------------------
    // execute_async — 对应 Java CommandBatchService.executeAsync()（IN_MEMORY 路径）
    // --------------------------------------------------------

    /// 将内存队列中所有命令通过 fred pipeline 一次性发往 Redis，
    /// 并把结果通过 oneshot 回填给各调用方。
    ///
    /// 对应 Java executeAsync() 在 !isRedisBasedQueue() 时的路径：
    ///   - IN_MEMORY        : 所有命令打包进 pipeline，一次 RTT 完成
    ///   - IN_MEMORY_ATOMIC : pipeline 头尾插入 MULTI / EXEC，保证原子性；
    ///                        EXEC 返回的结果数组按序回填给各调用方
    ///
    /// 末尾若 BatchOptions.sync_slaves > 0，额外发送 WAIT 或 WAITAOF 等待从库同步，
    /// 对应 Java executeAsync() 里 entry.add(waitCommand) 逻辑。
    ///
    /// # 调用方式
    /// async_execute() 入队后会挂起等待 oneshot，因此必须与 execute_async() 并发运行：
    /// ```rust
    /// let fut = service.async_execute(cmd);   // 入队，挂起
    /// service.execute_async().await?;          // 触发执行，唤醒 fut
    /// let value = fut.await?;                  // 取结果
    /// ```
    pub async fn execute_async(&self) -> anyhow::Result<()> {
        let pool    = &self.inner.connection_manager.pool;
        let options = self.inner.build_options();

        match self.options.get_execution_mode() {
            // ── REDIS_READ/WRITE_ATOMIC 路径 ──────────────────────
            // 对应 Java CommandBatchService.executeRedisBasedQueue()。
            // pipeline 和 txs 已在 async_execute() 里逐条建立，
            // 这里只需追加 EXEC，一次 try_all() 取结果。
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic => {
                let mut pg  = self.queued_pipeline.lock().await;
                let mut tg  = self.queued_txs.lock().await;

                let pipeline = match pg.take() {
                    Some(p) => p,
                    None    => return Ok(()), // 没有任何命令入队
                };
                let txs = std::mem::take(&mut *tg);

                // 队尾追加 EXEC，路由到与 MULTI 相同的固定 slot
                let _: Value = pipeline.custom(
                    CustomCommand::new_static("EXEC", ClusterHash::Custom(self.queued_slot), false),
                    Vec::<Value>::new(),
                ).await?;

                // 一次 RTT 把 MULTI + CMDs + EXEC 全部发出
                let mut results = pipeline.try_all::<Value>().await;

                // results 布局：[OK(MULTI), QUEUED × n, Array(EXEC results)]
                Self::distribute_exec_results(results.pop(), txs)?;

                // sync_slaves
                Self::wait_sync(self, pool, &options).await?;
            }

            // ── IN_MEMORY / IN_MEMORY_ATOMIC 路径 ─────────────────
            // 对应 Java CommandBatchService.executeAsync() !isRedisBasedQueue() 分支。
            mode => {
                let entries: Vec<BatchEntry> =
                    std::mem::take(&mut *self.queue.lock().unwrap());
                if entries.is_empty() {
                    return Ok(());
                }

                let is_atomic = matches!(mode, ExecutionMode::InMemoryAtomic);
                let (cmds, txs): (Vec<RedisCommand>, Vec<_>) =
                    entries.into_iter().map(|e| (e.cmd, e.tx)).unzip();

                let pipeline = pool.next().pipeline();

                // IN_MEMORY_ATOMIC：队头插 MULTI（ClusterHash::FirstKey 即可，非集群无影响）
                // 对应 Java: entry.addFirstCommand(MULTI)
                if is_atomic {
                    let _: Value = pipeline.custom(
                        CustomCommand::new_static("MULTI", ClusterHash::FirstKey, false),
                        Vec::<Value>::new(),
                    ).await?;
                }

                for cmd in cmds {
                    let _ = cmd.execute_on(&pipeline).await?;
                }

                // IN_MEMORY_ATOMIC：队尾追加 EXEC
                // 对应 Java: entry.add(EXEC)
                if is_atomic {
                    let _: Value = pipeline.custom(
                        CustomCommand::new_static("EXEC", ClusterHash::FirstKey, false),
                        Vec::<Value>::new(),
                    ).await?;
                }

                let mut results = pipeline.try_all::<Value>().await;

                if is_atomic {
                    Self::distribute_exec_results(results.pop(), txs)?;
                } else {
                    // IN_MEMORY：results 与 txs 一一对应
                    for (tx, result) in txs.into_iter().zip(results.into_iter()) {
                        let _ = tx.send(result.map_err(anyhow::Error::from));
                    }
                }

                Self::wait_sync(self, pool, &options).await?;
            }
        }

        Ok(())
    }

    // ── 辅助：EXEC 结果分发 ────────────────────────────────────────

    /// EXEC 返回值分发给各调用方 oneshot。
    /// results 布局：[OK(MULTI), QUEUED×n, Array(EXEC)] → pop() 取最后一个即 EXEC 结果。
    fn distribute_exec_results(
        exec_result: Option<Result<Value, fred::error::Error>>,
        txs: Vec<oneshot::Sender<anyhow::Result<Value>>>,
    ) -> anyhow::Result<()> {
        match exec_result {
            None => return Err(anyhow::anyhow!("pipeline 结果为空，EXEC 响应缺失")),
            Some(Ok(Value::Array(actual))) => {
                for (tx, val) in txs.into_iter().zip(actual.into_iter()) {
                    let _ = tx.send(Ok(val));
                }
            }
            Some(Ok(Value::Null)) => {
                // EXEC 返回 nil：事务被 WATCH 打断
                for tx in txs {
                    let _ = tx.send(Err(anyhow::anyhow!("EXEC 返回 nil：事务被 WATCH 打断")));
                }
            }
            Some(Ok(other)) => {
                return Err(anyhow::anyhow!("EXEC 响应格式异常: {:?}", other));
            }
            Some(Err(e)) => {
                // EXEC 本身失败（如 EXECABORT）
                for tx in txs {
                    let _ = tx.send(Err(anyhow::anyhow!("{}", e)));
                }
            }
        }
        Ok(())
    }

    // ── 辅助：批次末尾 WAIT / WAITAOF ─────────────────────────────

    /// 对应 Java executeAsync() 里 entry.add(waitCommand) 逻辑。
    async fn wait_sync(
        &self,
        pool: &fred::prelude::Pool,
        options: &fred::types::config::Options,
    ) -> anyhow::Result<()> {
        if self.options.get_sync_slaves() > 0 {
            let cmd = if self.options.is_sync_aof() {
                RedisCommand::WaitAof {
                    numlocal:    self.options.get_sync_locals() as i64,
                    numreplicas: self.options.get_sync_slaves() as i64,
                    timeout:     self.options.get_sync_timeout().as_millis() as i64,
                }
            } else {
                RedisCommand::Wait {
                    numreplicas: self.options.get_sync_slaves() as i64,
                    timeout:     self.options.get_sync_timeout().as_millis() as i64,
                }
            };
            cmd.execute(pool, options).await?;
        }
        Ok(())
    }
}

// ============================================================
// CommandAsyncServiceLike impl
// ============================================================

impl CommandAsyncServiceLike for CommandBatchService {
    fn inner(&self) -> &CommandAsyncInner {
        &self.inner
    }

    /// 对应 Java CommandBatchService.async() 覆写。
    ///
    /// **IN_MEMORY / IN_MEMORY_ATOMIC**：
    ///   将命令和 oneshot::Sender 压入内存队列，挂起等待 execute_async() 回填结果。
    ///
    /// **REDIS_READ/WRITE_ATOMIC**（对应 Java RedisQueuedBatchExecutor）：
    ///   持有 tokio::sync::Mutex 跨 await，保证 MULTI 先于所有用户命令入队：
    ///   1. 首条命令时懒建 pipeline 并入队 MULTI（用固定 slot 路由到同一集群节点）
    ///   2. 将用户命令入队到同一 pipeline
    ///   3. 挂起等待 execute_async() 发 EXEC 后回填结果
    async fn async_execute(&self, cmd: RedisCommand) -> anyhow::Result<Value> {
        let (tx, rx) = oneshot::channel();

        if self.options.is_redis_based_queue() {
            // 持锁跨 await，确保 MULTI → CMD 的入队顺序不被并发打乱
            let mut pg = self.queued_pipeline.lock().await;
            let mut tg = self.queued_txs.lock().await;

            let is_first = pg.is_none();
            let pipeline = pg.get_or_insert_with(|| {
                self.inner.connection_manager.pool.next().pipeline()
            });

            // 第一条命令前先入队 MULTI，路由到固定 slot（对应 Java connectionEntry.isFirstCommand()）
            if is_first {
                let _: Value = pipeline.custom(
                    CustomCommand::new_static("MULTI", ClusterHash::Custom(self.queued_slot), false),
                    Vec::<Value>::new(),
                ).await?;
            }

            // 用户命令入队（execute_on 在 pipeline 上调用，返回占位值，忽略）
            let _ = cmd.execute_on(pipeline).await?;
            tg.push(tx);
        } else {
            // IN_MEMORY / IN_MEMORY_ATOMIC：入队后挂起，等待 execute_async() 回填结果
            self.queue.lock().unwrap().push(BatchEntry { cmd, tx });
        }

        // 挂起，等待 execute_async() 调用 tx.send() 后唤醒
        rx.await.map_err(|_| anyhow::anyhow!("batch channel 已关闭（execute_async 未被调用即丢弃）"))?
    }

    fn is_eval_cache_active(&self) -> bool {
        false
    }

    fn is_batch(&self) -> bool {
        true
    }
}
