use crate::api::batch_options::{BatchOptions, ExecutionMode};
use crate::api::batch_result::BatchResult;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::command_async_service::{CommandAsyncInner, CommandAsyncServiceLike};
use crate::config::read_mode::ReadMode;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use fred::clients::{Client, Pipeline, Replicas};
use fred::interfaces::ClientLike;
use fred::types::{ClusterHash, CustomCommand, Value};
use fred::util::redis_keyslot;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

// ============================================================
// BatchCommandData — 对应 Java BatchCommandData
// ============================================================

/// 对应 Java BatchCommandData：命令与其结果 promise 封装在同一对象里。
/// 适用于所有执行模式，避免命令列表与结果列表分离导致的双锁问题。
struct BatchCommandData {
    cmd:   RedisCommand,
    tx:    oneshot::Sender<anyhow::Result<Value>>,
    /// 对应 Java BatchCommandData.index：全局入队序号，用于 BatchResult 按原始顺序返回响应。
    index: usize,
}

// ============================================================
// Entry — 对应 Java CommandBatchService.Entry 内部类
// ============================================================

/// 对应 Java CommandBatchService.Entry。
/// 每个 MasterSlaveEntry（节点）对应一个 Entry，存放发往该节点的所有命令。
/// read_only 默认 true，有写命令加入时置 false，用于决定是否可走副本路由。
struct Entry {
    commands:      Vec<BatchCommandData>,
    read_only:     bool,
}

impl Entry {
    fn new() -> Self {
        Self { commands: Vec::new(), read_only: true }
    }

    /// 对应 Java Entry.addCommand()：追加命令，写命令时置 read_only=false。
    fn add_command(&mut self, data: BatchCommandData) {
        if !data.cmd.is_read_only() {
            self.read_only = false;
        }
        self.commands.push(data);
    }

    /// 对应 Java Entry.addFirstCommand()：插入队头（用于 MULTI / CLIENT_REPLY_OFF）。
    fn add_first(&mut self, data: BatchCommandData) {
        self.commands.insert(0, data);
    }

    /// 对应 Java Entry.add()：追加到队尾（用于 EXEC / CLIENT_REPLY_ON / WAIT）。
    fn add(&mut self, data: BatchCommandData) {
        self.commands.push(data);
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }
}

// ============================================================
// AnyPipeline / QueuedState — REDIS_*_ATOMIC 模式下的原子状态
// ============================================================

/// 对应 Java RedisQueuedBatchExecutor.getConnection() 的两条路径：
/// REDIS_WRITE_ATOMIC → master（Pipeline<Client>）
/// REDIS_READ_ATOMIC  → replica（Pipeline<Replicas<Client>>）
enum AnyPipeline {
    Write(Pipeline<Client>),
    Read(Pipeline<Replicas<Client>>),
}

/// pipeline（命令）与 txs（result channel）合并在同一把锁下，
/// 保证两者始终一一对应，消除双锁风险。
/// cmd 已在 async_execute() 时 move 进 pipeline，此处只保留对应的 tx。
/// slot 在第一条命令入队时从其 routing_key 动态计算，用于 MULTI/EXEC 的 cluster 路由。
struct QueuedState {
    pipeline: Option<AnyPipeline>,
    txs:      Vec<oneshot::Sender<anyhow::Result<Value>>>,
    /// 对应 Java BatchCommandData.index：每条命令的全局入队序号
    indexes:  Vec<usize>,
    slot:     Option<u16>,
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

    // ── 对应 Java CommandBatchService.retryAttempts / retryDelay ──
    // 与父类同名字段语义不同：这两个字段是 batch 级别的节点解析重试预算，
    // 优先取 BatchOptions，未设置时回退到全局配置。
    // 执行时按已消耗次数扣减后传给底层 executor，实现两级重试共享预算。
    batch_retry_attempts: u32,
    batch_retry_delay:    Duration,

    // ── 对应 Java CommandBatchService.executed ─────────────
    /// 防止 execute_async / discard_async 被调用两次。
    executed: AtomicBool,

    // ── 对应 Java Entry.index（全局入队计数器）───────────
    /// 每条命令入队时递增，用于 BatchResult 按原始顺序返回响应。
    next_index: AtomicUsize,

    // ── IN_MEMORY / IN_MEMORY_ATOMIC 字段 ──────────────────
    /// 对应 Java CommandBatchService.commands（NodeSource → Entry 的 Map，此处简化为线性队列）。
    /// Rust 用 Mutex<Vec> 保证插入顺序，不需要 sortCommands。
    queue: Mutex<Vec<BatchCommandData>>,

    // ── REDIS_READ/WRITE_ATOMIC 字段 ───────────────────────
    /// pipeline 与 entries 合并在同一把锁下，对应 Java Entry.commands（BatchCommandData 同时持有命令和 promise）。
    /// 用 tokio::sync::Mutex 以便在持锁期间跨 await，保证 MULTI 先于所有用户命令入队。
    /// slot 在第一条命令入队时动态确定，避免构造时固定导致 cluster 拓扑变化后路由错误。
    queued: tokio::sync::Mutex<QueuedState>,

    // ── 对应 Java CommandBatchService.nestedServices ────────
    /// 嵌套 batch 追踪：key 是外部关注的结果 future（rx），value 是需要一起执行的子 batch 服务列表。
    /// 在 execute_async 触发时统一执行所有子服务并等待其完成。
    nested_services: Mutex<Vec<(oneshot::Receiver<anyhow::Result<Value>>, Vec<Arc<CommandBatchService>>)>>,
}

impl CommandBatchService {
    /// 对应 Java new CommandBatchService(ConnectionManager connectionManager, BatchOptions options)。
    ///
    /// retry_attempts 优先取 options，-1 时置 None（交给 fred 全局配置）。
    /// response_timeout 优先取 options，未设置时置 None（交给∂ fred 全局配置）。
    /// retryDelay 暂不处理（BatchOptions 尚未携带该字段）。
    pub fn new(connection_manager: Arc<FredConnectionManager>, options: BatchOptions) -> Self {
        let cfg = connection_manager.config();

        // 对应 Java CommandBatchService 构造器里的 resolve 逻辑：
        // 优先取 BatchOptions，未设置则回退全局配置。
        let retry_attempts = if options.get_retry_attempts() >= 0 {
            Some(options.get_retry_attempts() as u32)
        } else {
            None // fred 使用全局配置
        };
        let response_timeout = options.get_response_timeout();

        let batch_retry_attempts = if options.get_retry_attempts() >= 0 {
            options.get_retry_attempts() as u32
        } else {
            cfg.retry_attempts
        };
        let batch_retry_delay = options.get_retry_delay()
            .unwrap_or_else(|| Duration::from_millis(cfg.retry_delay_ms));

        Self {
            inner: CommandAsyncInner::new_all_params(
                connection_manager,
                retry_attempts,
                response_timeout,
                false,
            ),
            options,
            batch_retry_attempts,
            batch_retry_delay,
            executed: AtomicBool::new(false),
            next_index: AtomicUsize::new(0),
            queue:  Mutex::new(Vec::new()),
            queued: tokio::sync::Mutex::new(QueuedState { pipeline: None, txs: Vec::new(), indexes: Vec::new(), slot: None }),
            nested_services: Mutex::new(Vec::new()),
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

    /// 对应 Java CommandBatchService.isExecuted()
    pub fn is_executed(&self) -> bool {
        self.executed.load(Ordering::Acquire)
    }

    /// 对应 Java CommandBatchService.add(CompletableFuture, List<CommandBatchService>)。
    /// 注册嵌套 batch 服务：execute_async 触发时会先执行这些子服务，
    /// 并等待对应的 rx 收到结果后再汇总到本次批次的返回值中。
    pub fn add(
        &self,
        rx: oneshot::Receiver<anyhow::Result<Value>>,
        services: Vec<Arc<CommandBatchService>>,
    ) {
        self.nested_services.lock().push((rx, services));
    }

    /// 对应 Java CommandBatchService.executeAsyncVoid()。
    /// Java 实现：executeAsync().thenApply(res -> null)。
    /// 适用于 fire-and-forget 场景，调用方不关心批次结果。
    pub async fn execute_async_void(&self) -> anyhow::Result<()> {
        let _ = self.execute_async_inner().await?;
        Ok(())
    }

    /// Arc 版本：future 拥有 Arc，不借用外部变量，可安全存入 FuturesUnordered 或作为独立任务执行。
    pub async fn execute_async_void_owned(self: Arc<Self>) -> anyhow::Result<()> {
        let _ = self.execute_async_inner().await?;
        Ok(())
    }

    /// 对应 Java CommandBatchService.discardAsync()。
    ///
    /// IN_MEMORY / IN_MEMORY_ATOMIC：
    ///   清空内存队列，向所有等待的调用方回填 Err，防止 rx 永远 hang。
    ///
    /// REDIS_READ/WRITE_ATOMIC：
    ///   向 Redis 发 DISCARD 回滚已入队的事务命令，同样回填 Err。
    pub async fn discard_async(&self) -> anyhow::Result<()> {
        if self.executed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            anyhow::bail!("Batch already executed!");
        }
        let err = || anyhow::anyhow!("batch 已被 discard");

        if self.options.is_redis_based_queue() {
            let mut state = self.queued.lock().await;
            let txs = std::mem::take(&mut state.txs);
            let pipeline = state.pipeline.take();
            drop(state);

            // 向 Redis 发 DISCARD 取消事务
            if let Some(p) = pipeline {
                let slot = {
                    let s = self.queued.lock().await;
                    s.slot.unwrap_or(0)
                };
                let discard_cmd = CustomCommand::new_static(
                    "DISCARD",
                    ClusterHash::Custom(slot),
                    false,
                );
                let _ = match p {
                    AnyPipeline::Write(p) => {
                        let _: Value = p.custom(discard_cmd, Vec::<Value>::new()).await?;
                        p.try_all::<Value>().await
                    }
                    AnyPipeline::Read(p) => {
                        let _: Value = p.custom(discard_cmd, Vec::<Value>::new()).await?;
                        p.try_all::<Value>().await
                    }
                };
            }

            for tx in txs {
                let _ = tx.send(Err(err()));
            }
        } else {
            let entries: Vec<BatchCommandData> = std::mem::take(&mut *self.queue.lock());
            for entry in entries {
                let _ = entry.tx.send(Err(err()));
            }
        }

        Ok(())
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
    pub fn execute_async(&self) -> Pin<Box<dyn Future<Output = anyhow::Result<BatchResult>> + '_>> {
        Box::pin(self.execute_async_inner())
    }

    async fn execute_async_inner(&self) -> anyhow::Result<BatchResult> {
        if self.executed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            anyhow::bail!("Batch already executed!");
        }

        // 对应 Java executeRedisBasedQueue() 里的 responseTimeout 超时定时器：
        // BatchOptions.responseTimeout 优先，否则回退全局 command_timeout_ms。
        let timeout = self.options.get_response_timeout()
            .unwrap_or_else(|| Duration::from_millis(self.inner.connection_manager.config().command_timeout_ms));

        let result = tokio::time::timeout(timeout, self.execute_async_core()).await
            .map_err(|_| anyhow::anyhow!("Batch execution timed out after {:?}", timeout))?;

        result
    }

    /// execute_async_inner 的核心逻辑，被 timeout 包装。
    async fn execute_async_core(&self) -> anyhow::Result<BatchResult> {
        let pool = &self.inner.connection_manager.pool;

        // 使用 FuturesUnordered<LocalBoxFuture> 而非 JoinSet：
        // execute_async_void_owned 的 future 不是 Send（递归 Send 循环），JoinSet::spawn 不适用。
        // 对于 Redis I/O 密集型场景，同一 task 内交替 poll 与多 task 并发在网络层效果等价。
        use futures::StreamExt as _;
        let nested: Vec<(oneshot::Receiver<anyhow::Result<Value>>, Vec<Arc<CommandBatchService>>)> =
            std::mem::take(&mut *self.nested_services.lock());
        let mut nested_futs: futures::stream::FuturesUnordered<
            futures::future::LocalBoxFuture<'static, anyhow::Result<Value>>
        > = futures::stream::FuturesUnordered::new();
        for (rx, services) in nested {
            nested_futs.push(Box::pin(async move {
                // 内层：同组子服务并发触发，对应 Java executor.execute() 非阻塞发起
                let mut inner: futures::stream::FuturesUnordered<_> = services
                    .into_iter()
                    .map(|svc| CommandBatchService::execute_async_void_owned(svc))
                    .collect();
                while let Some(r) = inner.next().await {
                    r?;
                }
                // 子服务全部完成后等待 rx，对应 Java entry.getKey().whenComplete()
                rx.await.map_err(|_| anyhow::anyhow!("nested service rx 已关闭"))?
            }));
        }

        match self.options.get_execution_mode() {
            // ── REDIS_READ/WRITE_ATOMIC 路径 ──────────────────────
            // 对应 Java executeRedisBasedQueue()：nested services 先于 EXEC 完成（nestedServicesFuture.whenComplete → EXEC）
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic => {
                // 对应 Java nestedServicesFuture.whenComplete：先等所有 nested services 完成，再发 EXEC
                let mut nested_responses: Vec<Value> = Vec::new();
                while let Some(res) = nested_futs.next().await {
                    nested_responses.push(res?);
                }

                let mut state = self.queued.lock().await;
                let pipeline = match state.pipeline.take() {
                    Some(p) => p,
                    None    => return Ok(BatchResult::new(nested_responses, 0)),
                };
                let txs       = std::mem::take(&mut state.txs);
                let indexes   = std::mem::take(&mut state.indexes);
                let exec_slot = state.slot.unwrap_or(0);
                drop(state);

                let sync_slaves_opt = self.options.get_sync_slaves();
                let sync_aof        = self.options.is_sync_aof();
                let sync_locals     = self.options.get_sync_locals();
                let sync_timeout    = self.options.get_sync_timeout().as_millis() as i64;

                let exec_cmd = CustomCommand::new_static("EXEC", ClusterHash::Custom(exec_slot), false);

                // 队尾追加 EXEC，再追加 WAIT/WAITAOF，一次 try_all() 全部发出
                let mut results = match pipeline {
                    AnyPipeline::Write(p) => {
                        let _: Value = p.custom(exec_cmd, Vec::<Value>::new()).await?;
                        if sync_slaves_opt > 0 {
                            let (cmd, args) = Self::make_wait_cmd(exec_slot, sync_aof, sync_locals, sync_slaves_opt, sync_timeout);
                            let _: Value = p.custom(cmd, args).await?;
                        }
                        p.try_all::<Value>().await
                    }
                    AnyPipeline::Read(p) => {
                        let _: Value = p.custom(exec_cmd, Vec::<Value>::new()).await?;
                        if sync_slaves_opt > 0 {
                            let (cmd, args) = Self::make_wait_cmd(exec_slot, sync_aof, sync_locals, sync_slaves_opt, sync_timeout);
                            let _: Value = p.custom(cmd, args).await?;
                        }
                        p.try_all::<Value>().await
                    }
                };

                let mut synced_slaves: i64 = 0;
                if sync_slaves_opt > 0 {
                    synced_slaves = Self::extract_synced_slaves(results.pop());
                }

                let mut all_indexed: Vec<(usize, Value)> = Vec::new();
                if self.options.is_skip_result() {
                    for tx in txs {
                        let _ = tx.send(Ok(Value::Null));
                    }
                } else {
                    let exec_result = results.pop();
                    Self::distribute_exec_results(exec_result.clone(), txs)?;
                    Self::collect_exec_indexed(exec_result, indexes, &mut all_indexed);
                }

                all_indexed.sort_by_key(|(i, _)| *i);
                let mut responses: Vec<Value> = all_indexed.into_iter().map(|(_, v)| v).collect();
                responses.extend(nested_responses);
                Ok(BatchResult::new(responses, synced_slaves))
            }

            // ── IN_MEMORY / IN_MEMORY_ATOMIC 路径 ─────────────────
            // 对应 Java executeAsync() !isRedisBasedQueue()：
            // slots 同时计数主 batch 节点 + nested services，两者并发，slots 归零后完成。
            mode => {
                let entries: Vec<BatchCommandData> = std::mem::take(&mut *self.queue.lock());
                if entries.is_empty() && nested_futs.is_empty() {
                    return Ok(BatchResult::new(Vec::new(), 0));
                }

                let is_atomic       = matches!(mode, ExecutionMode::InMemoryAtomic);
                let is_skip         = self.options.is_skip_result();
                let sync_slaves_opt = self.options.get_sync_slaves();
                let sync_aof        = self.options.is_sync_aof();
                let sync_locals     = self.options.get_sync_locals();
                let sync_timeout    = self.options.get_sync_timeout().as_millis() as i64;

                let mut join_set: tokio::task::JoinSet<anyhow::Result<(i64, Vec<(usize, Value)>)>> =
                    tokio::task::JoinSet::new();

                if !entries.is_empty() {
                    let mut result: HashMap<MasterSlaveEntry, Entry> = HashMap::new();
                    for data in entries {
                        let slot = data.cmd.routing_key().map(redis_keyslot).unwrap_or(0);
                        let mse = self.resolve_entry(slot).await?;
                        result.entry(mse).or_insert_with(Entry::new).add_command(data);
                    }
                    self.load_scripts(&mut result).await?;

                    let pool_owned = pool.clone();
                    let read_mode  = self.inner.connection_manager.read_mode.clone();
                    for (mse, entry) in result {
                        let pool_c    = pool_owned.clone();
                        let read_mode = read_mode.clone();
                        let slot      = mse.slot().unwrap_or(0);
                        join_set.spawn(async move {
                            let use_replica = !is_atomic
                                && entry.read_only
                                && !matches!(read_mode, ReadMode::Master);

                            let indexes: Vec<usize> = entry.commands.iter().map(|d| d.index).collect();
                            let (cmds, txs): (Vec<RedisCommand>, Vec<_>) =
                                entry.commands.into_iter().map(|d| (d.cmd, d.tx)).unzip();

                            let pipeline: AnyPipeline = if use_replica {
                                AnyPipeline::Read(pool_c.replicas().pipeline())
                            } else {
                                AnyPipeline::Write(pool_c.next().pipeline())
                            };

                            if is_atomic {
                                let multi = CustomCommand::new_static("MULTI", ClusterHash::Custom(slot), false);
                                match &pipeline {
                                    AnyPipeline::Write(p) => { let _: Value = p.custom(multi, Vec::<Value>::new()).await?; }
                                    AnyPipeline::Read(p)  => { let _: Value = p.custom(multi, Vec::<Value>::new()).await?; }
                                }
                            }
                            for cmd in cmds {
                                match &pipeline {
                                    AnyPipeline::Write(p) => { cmd.execute_on(p).await?; }
                                    AnyPipeline::Read(p)  => { cmd.execute_on(p).await?; }
                                }
                            }
                            if is_atomic {
                                let exec = CustomCommand::new_static("EXEC", ClusterHash::Custom(slot), false);
                                match &pipeline {
                                    AnyPipeline::Write(p) => { let _: Value = p.custom(exec, Vec::<Value>::new()).await?; }
                                    AnyPipeline::Read(p)  => { let _: Value = p.custom(exec, Vec::<Value>::new()).await?; }
                                }
                            }
                            if sync_slaves_opt > 0 {
                                let (wait_cmd, wait_args) = Self::make_wait_cmd(slot, sync_aof, sync_locals, sync_slaves_opt, sync_timeout);
                                match &pipeline {
                                    AnyPipeline::Write(p) => { let _: Value = p.custom(wait_cmd, wait_args).await?; }
                                    AnyPipeline::Read(p)  => { let _: Value = p.custom(wait_cmd, wait_args).await?; }
                                }
                            }

                            let mut results = match pipeline {
                                AnyPipeline::Write(p) => p.try_all::<Value>().await,
                                AnyPipeline::Read(p)  => p.try_all::<Value>().await,
                            };

                            let mut local_synced: i64 = 0;
                            if sync_slaves_opt > 0 {
                                local_synced = Self::extract_synced_slaves(results.pop());
                            }

                            let mut indexed: Vec<(usize, Value)> = Vec::new();
                            if is_skip {
                                for tx in txs { let _ = tx.send(Ok(Value::Null)); }
                            } else if is_atomic {
                                let exec_result = results.pop();
                                Self::distribute_exec_results(exec_result.clone(), txs)?;
                                Self::collect_exec_indexed(exec_result, indexes, &mut indexed);
                            } else {
                                for (i, (tx, result)) in indexes.into_iter().zip(txs.into_iter().zip(results.into_iter())) {
                                    let val = result.unwrap_or(Value::Null);
                                    let _ = tx.send(Ok(val.clone()));
                                    indexed.push((i, val));
                                }
                            }
                            Ok((local_synced, indexed))
                        });
                    }
                }

                // 对应 Java slots 同时计数主 batch 节点 + nested services，两者并发
                let (main_res, nested_res) = tokio::join!(
                    async {
                        let mut synced_slaves: i64 = 0;
                        let mut all_indexed: Vec<(usize, Value)> = Vec::new();
                        while let Some(res) = join_set.join_next().await {
                            let (s, idx) = res.map_err(|e| anyhow::anyhow!("batch task panicked: {e}"))??;
                            synced_slaves += s;
                            all_indexed.extend(idx);
                        }
                        Ok::<_, anyhow::Error>((synced_slaves, all_indexed))
                    },
                    async {
                        let mut responses: Vec<Value> = Vec::new();
                        while let Some(res) = nested_futs.next().await {
                            responses.push(res?);
                        }
                        Ok::<_, anyhow::Error>(responses)
                    }
                );

                let (synced_slaves, mut all_indexed) = main_res?;
                let nested_responses = nested_res?;

                all_indexed.sort_by_key(|(i, _)| *i);
                let mut responses: Vec<Value> = all_indexed.into_iter().map(|(_, v)| v).collect();
                responses.extend(nested_responses);
                Ok(BatchResult::new(responses, synced_slaves))
            }
        }
    }

    // ── 辅助：节点解析（带重试）────────────────────────────────────────

    /// 对应 Java resolveCommandsInMemory 里找不到节点时的重试逻辑。
    /// cluster failover 期间路由表可能短暂为空，重试等待路由表刷新后再查。
    async fn resolve_entry(&self, slot: u16) -> anyhow::Result<MasterSlaveEntry> {
        for attempt in 0..=self.batch_retry_attempts {
            if let Some(mse) = self.inner.connection_manager.get_write_entry(slot) {
                return Ok(mse);
            }
            if attempt < self.batch_retry_attempts {
                tokio::time::sleep(self.batch_retry_delay).await;
            }
        }
        anyhow::bail!("no server found for slot {slot} after {} attempts", self.batch_retry_attempts + 1)
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

    // ── 辅助：提取 syncedSlaves ──────────────────────────────────

    /// 从 pipeline 结果中提取 WAIT/WAITAOF 返回的同步节点数。
    /// WAIT 返回 Integer(slaves)；WAITAOF 返回 Array([locals, slaves])。
    fn extract_synced_slaves(wait_result: Option<Result<Value, fred::error::Error>>) -> i64 {
        match wait_result {
            Some(Ok(Value::Integer(n))) => n,
            Some(Ok(Value::Array(nums))) => {
                // WAITAOF: [local_num, slave_num]，取第二个元素
                nums.get(1).and_then(|v| match v {
                    Value::Integer(n) => Some(*n),
                    _ => None,
                }).unwrap_or(0)
            }
            _ => 0,
        }
    }

    // ── 辅助：EXEC 结果收集带 index ──────────────────────────────

    /// 从 EXEC 返回值中提取各命令响应，配合 indexes 填入 all_indexed。
    fn collect_exec_indexed(
        exec_result: Option<Result<Value, fred::error::Error>>,
        indexes: Vec<usize>,
        all_indexed: &mut Vec<(usize, Value)>,
    ) {
        match exec_result {
            Some(Ok(Value::Array(actual))) => {
                for (idx, val) in indexes.into_iter().zip(actual.into_iter()) {
                    all_indexed.push((idx, val));
                }
            }
            _ => {}
        }
    }

    // ── 对应 Java CommandBatchService.loadScripts() ───────────────

    /// 对应 Java CommandBatchService.loadScripts(Map<NodeSource, Entry> r)。
    ///
    /// 在 execute_async 主 pipeline 执行前调用：
    /// 1. 扫描各 slot 分组里的 Eval 命令，收集未在目标节点缓存过的脚本
    /// 2. 对每个唯一脚本，向目标节点发 SCRIPT LOAD（幂等，仅当未缓存时发送）
    /// 3. 把 Eval { script, keys, args } 原地替换成 EvalSha { sha, keys, args }
    /// 4. SCRIPT LOAD 成功后，把 sha 写入 SCRIPT_SHA_CACHE（per-node 缓存）
    ///
    /// 与 Java 的差异：
    /// - Java 对 read-only Entry 走 executeAllAsync（发给所有节点包括副本），
    ///   Rust 这边暂时只发给 primary（Redis 副本不自动同步脚本缓存，
    ///   read-only batch 的 EVALSHA 如果走副本会在首次出错后降级；后续可补）。
    async fn load_scripts(
        &self,
        result: &mut HashMap<MasterSlaveEntry, Entry>,
    ) -> anyhow::Result<()> {
        if !self.inner.connection_manager.config().use_script_cache {
            return Ok(());
        }

        let pool = &self.inner.connection_manager.pool;

        for (mse, entry) in result.iter_mut() {
            let slot = mse.slot().unwrap_or(0);
            let server = mse.get_client();

            let mut to_load: Vec<String> = Vec::new();
            for data in entry.commands.iter() {
                if let RedisCommand::Eval { script, .. } = &data.cmd {
                    let already_cached = ServiceManager::is_cached(server, script);
                    if !already_cached && !to_load.contains(script) {
                        to_load.push(script.clone());
                    }
                }
            }

            if !to_load.is_empty() {
                let client = pool.next().clone();
                for script in &to_load {
                    let args: Vec<Value> = vec!["LOAD".into(), script.clone().into()];
                    let cmd = CustomCommand::new_static("SCRIPT", ClusterHash::Custom(slot), false);
                    let _: Value = client.custom(cmd, args).await
                        .map_err(|e| anyhow::anyhow!("SCRIPT LOAD 失败: {e}"))?;
                }
                ServiceManager::cache_scripts(server, to_load);
            }

            for data in entry.commands.iter_mut() {
                let new_cmd = match &mut data.cmd {
                    RedisCommand::Eval { script, keys, args } => {
                        let sha = ServiceManager::calc_sha(script);
                        Some(RedisCommand::EvalSha {
                            sha,
                            keys: std::mem::take(keys),
                            args: std::mem::take(args),
                        })
                    }
                    _ => None,
                };
                if let Some(nc) = new_cmd {
                    data.cmd = nc;
                }
            }
        }

        Ok(())
    }

    // ── 辅助：isWaitCommand ────────────────────────────────────────

    /// 对应 Java CommandBatchService.isWaitCommand()。
    /// WAIT / WAITAOF 不应进入 REDIS_*_ATOMIC pipeline，在入队时拦截。
    fn is_wait_command(cmd: &RedisCommand) -> bool {
        matches!(cmd, RedisCommand::Wait { .. } | RedisCommand::WaitAof { .. })
    }

    // ── 辅助：构造 WAIT / WAITAOF pipeline 命令 ───────────────────

    /// 对应 Java entry.add(waitCommand)：返回可直接传入 pipeline.custom() 的命令和参数。
    fn make_wait_cmd(
        slot:        u16,
        sync_aof:    bool,
        sync_locals: u32,
        sync_slaves: u32,
        timeout_ms:  i64,
    ) -> (CustomCommand, Vec<Value>) {
        if sync_aof {
            let cmd  = CustomCommand::new_static("WAITAOF", ClusterHash::Custom(slot), false);
            let args = vec![
                Value::Integer(sync_locals as i64),
                Value::Integer(sync_slaves as i64),
                Value::Integer(timeout_ms),
            ];
            (cmd, args)
        } else {
            let cmd  = CustomCommand::new_static("WAIT", ClusterHash::Custom(slot), false);
            let args = vec![
                Value::Integer(sync_slaves as i64),
                Value::Integer(timeout_ms),
            ];
            (cmd, args)
        }
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
            // 对应 Java CommandBatchService.isWaitCommand()：
            // WAIT / WAITAOF 不入队到 Redis pipeline，否则会产生协议错误。
            // Java 在 executeRedisBasedQueue 里过滤；Rust 在入队时直接拦截。
            if Self::is_wait_command(&cmd) {
                let _ = tx.send(Err(anyhow::anyhow!(
                    "WAIT/WAITAOF 不支持在 REDIS_*_ATOMIC 模式下使用"
                )));
                return rx.await.map_err(|_| anyhow::anyhow!("channel closed"))?;
            }

            // 持锁跨 await，确保 MULTI → CMD 的入队顺序不被并发打乱
            let mut state = self.queued.lock().await;

            let is_first = state.pipeline.is_none();
            let pool = &self.inner.connection_manager.pool;

            // 对应 Java RedisQueuedBatchExecutor.getConnection()：
            // REDIS_READ_ATOMIC → replica；REDIS_WRITE_ATOMIC → master
            if is_first {
                // 从第一条命令的 routing_key 动态计算 slot，避免构造时固定导致集群拓扑变化后路由错误。
                // 对应 Java RedisQueuedBatchExecutor 通过 NodeSource(slot) 寻址。
                if let Some(key_bytes) = cmd.routing_key() {
                    state.slot = Some(redis_keyslot(key_bytes));
                }

                state.pipeline = Some(
                    if matches!(self.options.get_execution_mode(), ExecutionMode::RedisReadAtomic) {
                        AnyPipeline::Read(pool.replicas().pipeline())
                    } else {
                        AnyPipeline::Write(pool.next().pipeline())
                    }
                );
            }

            // 第一条命令前先入队 MULTI，路由到动态计算的 slot（对应 Java connectionEntry.isFirstCommand()）
            let multi_slot = state.slot.unwrap_or(0);
            let multi_cmd = CustomCommand::new_static("MULTI", ClusterHash::Custom(multi_slot), false);
            match state.pipeline.as_mut().unwrap() {
                AnyPipeline::Write(p) => {
                    if is_first { let _: Value = p.custom(multi_cmd, Vec::<Value>::new()).await?; }
                    let _ = cmd.execute_on(p).await?;
                }
                AnyPipeline::Read(p) => {
                    if is_first { let _: Value = p.custom(multi_cmd, Vec::<Value>::new()).await?; }
                    let _ = cmd.execute_on(p).await?;
                }
            }
            state.txs.push(tx);
            let idx = self.next_index.fetch_add(1, Ordering::Relaxed);
            state.indexes.push(idx);
        } else {
            // IN_MEMORY / IN_MEMORY_ATOMIC：入队后挂起，等待 execute_async() 回填结果
            let idx = self.next_index.fetch_add(1, Ordering::Relaxed);
            self.queue.lock().push(BatchCommandData { cmd, tx, index: idx });
        }

        // 挂起，等待 execute_async() 调用 tx.send() 后唤醒
        rx.await.map_err(|_| anyhow::anyhow!("batch channel 已关闭（execute_async 未被调用即丢弃）"))?
    }

    /// 对应 Java CommandBatchService.isEvalCacheActive() 覆写。
    /// Java 在 batch 模式下始终返回 false：batch 命令是批量一次性发的，
    /// 不存在单条 EVALSHA 失败后降级回 EVAL 的机会（父类的 eval 缓存回调机制不适用）。
    fn is_eval_cache_active(&self) -> bool {
        false
    }

    fn is_batch(&self) -> bool {
        true
    }
}

