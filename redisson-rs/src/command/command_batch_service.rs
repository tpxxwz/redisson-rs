use crate::api::batch_options::{BatchOptions, ExecutionMode};
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::command_async_service::{CommandAsyncInner, CommandAsyncServiceLike};
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
use std::future::Future;
use std::pin::Pin;
use fred::clients::{Client, Pipeline, Replicas};
use fred::interfaces::ClientLike;
use fred::types::{ClusterHash, CustomCommand, Value};
use fred::util::redis_keyslot;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

// ============================================================
// BatchCommandData — 对应 Java BatchCommandData
// ============================================================

/// 对应 Java BatchCommandData：命令与其结果 promise 封装在同一对象里。
/// 适用于所有执行模式，避免命令列表与结果列表分离导致的双锁问题。
struct BatchCommandData {
    cmd: RedisCommand,
    tx:  oneshot::Sender<anyhow::Result<Value>>,
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

    // ── 对应 Java CommandBatchService.executed ─────────────
    /// 防止 execute_async / discard_async 被调用两次。
    executed: AtomicBool,

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

        Self {
            inner: CommandAsyncInner::new_all_params(
                connection_manager,
                retry_attempts,
                response_timeout,
                false,
            ),
            options,
            executed: AtomicBool::new(false),
            queue:  Mutex::new(Vec::new()),
            queued: tokio::sync::Mutex::new(QueuedState { pipeline: None, txs: Vec::new(), slot: None }),
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
        self.execute_async_inner().await
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
    pub fn execute_async(&self) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + '_>> {
        Box::pin(self.execute_async_inner())
    }

    async fn execute_async_inner(&self) -> anyhow::Result<()> {
        if self.executed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            anyhow::bail!("Batch already executed!");
        }

        let pool    = &self.inner.connection_manager.pool;
        let options = self.inner.build_options();

        match self.options.get_execution_mode() {
            // ── REDIS_READ/WRITE_ATOMIC 路径 ──────────────────────
            // 对应 Java CommandBatchService.executeRedisBasedQueue()。
            // pipeline 和 txs 已在 async_execute() 里逐条建立，
            // 这里只需追加 EXEC，一次 try_all() 取结果。
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic => {
                let mut state = self.queued.lock().await;

                let pipeline = match state.pipeline.take() {
                    Some(p) => p,
                    None    => return Ok(()), // 没有任何命令入队
                };
                let txs      = std::mem::take(&mut state.txs);
                let exec_slot = state.slot.unwrap_or(0);
                drop(state);
                let exec_cmd = CustomCommand::new_static("EXEC", ClusterHash::Custom(exec_slot), false);

                // 队尾追加 EXEC，一次 RTT 把 MULTI + CMDs + EXEC 全部发出
                let mut results = match pipeline {
                    AnyPipeline::Write(p) => {
                        let _: Value = p.custom(exec_cmd, Vec::<Value>::new()).await?;
                        p.try_all::<Value>().await
                    }
                    AnyPipeline::Read(p) => {
                        let _: Value = p.custom(exec_cmd, Vec::<Value>::new()).await?;
                        p.try_all::<Value>().await
                    }
                };

                // results 布局：[OK(MULTI), QUEUED × n, Array(EXEC results)]
                Self::distribute_exec_results(results.pop(), txs)?;

                Self::wait_sync(self, pool, &options).await?;
            }

            // ── IN_MEMORY / IN_MEMORY_ATOMIC 路径 ─────────────────
            // 对应 Java CommandBatchService.executeAsync() !isRedisBasedQueue() 分支。
            //
            // Java：按 NodeSource 分组后，AtomicInteger slots = r.size()，
            //   for each node → executor.execute()（非阻塞，Netty 事件循环并发），
            //   slots 归零时 voidPromise.complete()。
            //
            // Rust：按 routing_key() → redis_keyslot() 分组，每组 spawn 一个 task，
            //   全部 task 通过 JoinSet 并发执行，join_next() 等全部完成，对应 Java 的计数器语义。
            mode => {
                let entries: Vec<BatchCommandData> = std::mem::take(&mut *self.queue.lock());
                if entries.is_empty() {
                    return Ok(());
                }

                let is_atomic = matches!(mode, ExecutionMode::InMemoryAtomic);

                // 按 MasterSlaveEntry（主节点）分组，对应 Java NodeSource → Entry Map。
                // 同一主节点上的不同 slot 合并到同一 pipeline，避免多余 RTT。
                let mut node_groups: HashMap<
                    MasterSlaveEntry,
                    Vec<(RedisCommand, oneshot::Sender<anyhow::Result<Value>>)>,
                > = HashMap::new();
                for entry in entries {
                    let slot = entry.cmd.routing_key().map(redis_keyslot).unwrap_or(0);
                    let mse = self.inner.connection_manager.get_write_entry(slot)
                        .expect("no server configured");
                    node_groups.entry(mse).or_default().push((entry.cmd, entry.tx));
                }

                // loadScripts：EVAL → EVALSHA 替换，并对未缓存节点发 SCRIPT LOAD。
                // 对应 Java execute() 里 loadScripts(r) 的调用，必须在 pipeline spawn 之前完成。
                self.load_scripts(&mut node_groups).await?;

                // 各节点并发执行，对应 Java AtomicInteger slots + executor.execute() 非阻塞发起
                let pool_owned = pool.clone();
                let mut join_set: tokio::task::JoinSet<anyhow::Result<()>> =
                    tokio::task::JoinSet::new();

                for (mse, group) in node_groups {
                    let pool_c = pool_owned.clone();
                    // slot 用于 ClusterHash 路由，确保命令发往正确节点
                    let slot = mse.slot().unwrap_or(0);
                    join_set.spawn(async move {
                        let (cmds, txs): (Vec<RedisCommand>, Vec<_>) =
                            group.into_iter().unzip();

                        // pool_c.next() 返回 &Client，pipeline() 内部 clone client，
                        // Pipeline<Client> 完全 owned，不借 pool_c
                        let pipeline = pool_c.next().pipeline();

                        // IN_MEMORY_ATOMIC：组头注入 MULTI，对应 Java entry.addFirstCommand(MULTI)
                        if is_atomic {
                            let _: Value = pipeline.custom(
                                CustomCommand::new_static("MULTI", ClusterHash::Custom(slot), false),
                                Vec::<Value>::new(),
                            ).await?;
                        }

                        for cmd in cmds {
                            cmd.execute_on(&pipeline).await?;
                        }

                        // IN_MEMORY_ATOMIC：组尾追加 EXEC，对应 Java entry.add(EXEC)
                        if is_atomic {
                            let _: Value = pipeline.custom(
                                CustomCommand::new_static("EXEC", ClusterHash::Custom(slot), false),
                                Vec::<Value>::new(),
                            ).await?;
                        }

                        let mut results = pipeline.try_all::<Value>().await;

                        if is_atomic {
                            // results 布局：[OK(MULTI), QUEUED×n, Array(EXEC)]
                            Self::distribute_exec_results(results.pop(), txs)?;
                        } else {
                            for (tx, result) in txs.into_iter().zip(results.into_iter()) {
                                let _ = tx.send(result.map_err(anyhow::Error::from));
                            }
                        }
                        Ok(())
                    });
                }

                // 等待所有节点完成，对应 Java slots.decrementAndGet() == 0 触发 voidPromise
                while let Some(res) = join_set.join_next().await {
                    res.map_err(|e| anyhow::anyhow!("batch task panicked: {e}"))??;
                }

                Self::wait_sync(self, pool, &options).await?;
            }
        }

        // 对应 Java execute() 里 nestedServices 并发触发逻辑：
        // 取出所有嵌套服务，并发执行并等待对应的 rx 全部完成。
        self.execute_nested_services().await?;

        Ok(())
    }

    // ── 辅助：嵌套服务执行 ────────────────────────────────────────────

    /// 对应 Java execute() 里 nestedServices 的触发逻辑。
    /// 取出所有注册的嵌套服务组，顺序执行各组子服务并等待 rx 信号。
    ///
    /// Java 侧通过 AtomicInteger slots 实现并发等待，Rust 侧改为顺序执行：
    /// 子服务数量通常很少，顺序执行不会成为瓶颈；
    /// 避免 JoinSet::spawn 要求 Future: Send 导致的自引用问题。
    async fn execute_nested_services(&self) -> anyhow::Result<()> {
        let nested: Vec<(oneshot::Receiver<anyhow::Result<Value>>, Vec<Arc<CommandBatchService>>)> =
            std::mem::take(&mut *self.nested_services.lock());

        for (rx, services) in nested {
            for svc in &services {
                svc.execute_async().await?;
            }
            rx.await
                .map_err(|_| anyhow::anyhow!("nested service rx 已关闭"))??;
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
        node_groups: &mut HashMap<MasterSlaveEntry, Vec<(RedisCommand, oneshot::Sender<anyhow::Result<Value>>)>>,
    ) -> anyhow::Result<()> {
        if !self.inner.connection_manager.config().use_script_cache {
            return Ok(());
        }

        let pool = &self.inner.connection_manager.pool;

        for (mse, group) in node_groups.iter_mut() {
            let slot = mse.slot().unwrap_or(0);
            let server = mse.get_client();

            // script 原文 → SHA（仅当 server 已知时做 per-node 检查，否则每次都加载）
            let mut to_load: Vec<String> = Vec::new();
            for (cmd, _) in group.iter() {
                if let RedisCommand::Eval { script, .. } = cmd {
                    let already_cached = ServiceManager::is_cached(server, script);
                    if !already_cached && !to_load.contains(script) {
                        to_load.push(script.clone());
                    }
                }
            }

            // 向目标节点发 SCRIPT LOAD（通过 ClusterHash::Custom(slot) 路由到同一 primary）
            if !to_load.is_empty() {
                let client = pool.next().clone();
                for script in &to_load {
                    let args: Vec<Value> = vec!["LOAD".into(), script.clone().into()];
                    let cmd = CustomCommand::new_static("SCRIPT", ClusterHash::Custom(slot), false);
                    let _: Value = client.custom(cmd, args).await
                        .map_err(|e| anyhow::anyhow!("SCRIPT LOAD 失败: {e}"))?;
                }
                // 缓存已加载的脚本（per-node），对应 Java serviceManager.cacheScripts(addr, newShas)
                ServiceManager::cache_scripts(server, to_load);
            }

            // Eval → EvalSha 原地替换，对应 Java data.updateCommand(EVALSHA) + data.getParams()[0] = sha1
            for (cmd, _) in group.iter_mut() {
                let new_cmd = match cmd {
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
                    *cmd = nc;
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
        } else {
            // IN_MEMORY / IN_MEMORY_ATOMIC：入队后挂起，等待 execute_async() 回填结果
            self.queue.lock().push(BatchCommandData { cmd, tx });
        }

        // 挂起，等待 execute_async() 调用 tx.send() 后唤醒
        rx.await.map_err(|_| anyhow::anyhow!("batch channel 已关闭（execute_async 未被调用即丢弃）"))?
    }

    fn is_eval_cache_active(&self) -> bool {
        self.inner.connection_manager.config().use_script_cache
    }

    fn is_batch(&self) -> bool {
        true
    }
}
