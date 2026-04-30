use crate::api::batch_options::{BatchOptions, ExecutionMode};
use crate::api::batch_result::BatchResult;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::command_async_service::{CommandAsyncInner, CommandAsyncServiceLike};
use crate::config::read_mode::ReadMode;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
use std::time::Duration;
use fred::clients::{Client, Pipeline, Replicas};

use fred::interfaces::ClientLike;
use fred::types::{ClusterHash, CustomCommand, Value};
use fred::util::redis_keyslot;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

// ============================================================
// BatchCommandData — 对应 Java BatchCommandData
// ============================================================

/// 对应 Java BatchCommandData：命令与其结果 promise 封装在同一对象里。
struct BatchCommandData {
    cmd:   RedisCommand,
    tx:    oneshot::Sender<anyhow::Result<Value>>,
    /// 全局入队序号，用于 BatchResult 按原始顺序返回响应。
    index: usize,
}

// ============================================================
// Entry — 对应 Java CommandBatchService.Entry 内部类
// ============================================================

/// 对应 Java CommandBatchService.Entry。
/// 每个 MasterSlaveEntry（节点）对应一个 Entry，存放发往该节点的所有命令。
struct Entry {
    commands:  VecDeque<BatchCommandData>,
    read_only: bool,
}

impl Entry {
    fn new() -> Self {
        Self { commands: VecDeque::new(), read_only: true }
    }

    fn add_command(&mut self, data: BatchCommandData) {
        if !data.cmd.is_read_only() {
            self.read_only = false;
        }
        self.commands.push_back(data);
    }
}

// ============================================================
// AnyPipeline — IN_MEMORY 路径临时使用，不存入 struct
// ============================================================

enum AnyPipeline {
    Write(Pipeline<Client>),
    Read(Pipeline<Replicas<Client>>),
}

// ============================================================
// BatchState — 批次队列状态，构造时确定变体
// ============================================================

/// 批次队列的两种存储模式，一次只有一种处于活跃状态。
enum BatchState {
    /// IN_MEMORY / IN_MEMORY_ATOMIC：命令缓冲在本地，execute_async 时通过 pipeline 一次发出。
    InMemory(Vec<BatchCommandData>),
    /// REDIS_READ/WRITE_ATOMIC：每条命令通过 pool + ClusterHash::Custom(slot)
    /// 立即发往 Redis（MULTI 状态），同一 slot 保证路由到同一 cluster 节点。
    RedisBased {
        /// 第一条命令的 routing_key 计算得出，后续所有命令用此 slot 路由。
        slot:    Option<u16>,
        /// 按入队顺序存放每条命令的 result sender，execute 时分发 EXEC 结果。
        entries: Vec<(oneshot::Sender<anyhow::Result<Value>>, usize)>,
    },
}

// ============================================================
// CommandBatchService
// ============================================================

/// 对应 Java org.redisson.command.CommandBatchService。
pub struct CommandBatchService {
    pub(crate) inner:   CommandAsyncInner,
    pub(crate) options: BatchOptions,

    // 对应 Java CommandBatchService.retryAttempts / retryDelay（节点解析重试）
    batch_retry_attempts: u32,
    batch_retry_delay:    Duration,

    /// 防止 execute_async / discard_async 被调用两次。
    executed: AtomicBool,

    /// 每条命令入队时递增，用于 BatchResult 按原始顺序返回响应。
    next_index: AtomicUsize,

    /// 批次队列状态，构造时根据 ExecutionMode 确定变体。
    state: tokio::sync::Mutex<BatchState>,

    // Java CommandBatchService.nestedServices 对应的机制在此处不实现。
    // 该机制专为 Live Object 的高级 batch 场景设计（企业版及未来版本使用），
    // 在开源版 redisson-4.3.0 中 add() 方法从未有外部调用方。
    // Rust 侧不实现 Live Object（依赖 Java 动态代理 / ByteBuddy，Rust 无法复现），
    // 因此整套 nestedServices 逻辑均不需要。
}

impl CommandBatchService {
    /// 对应 Java new CommandBatchService(ConnectionManager connectionManager, BatchOptions options)。
    pub fn new(connection_manager: Arc<FredConnectionManager>, options: BatchOptions) -> Self {
        let cfg = connection_manager.config();

        let retry_attempts = if options.get_retry_attempts() >= 0 {
            Some(options.get_retry_attempts() as u32)
        } else {
            None
        };
        let response_timeout = options.get_response_timeout();

        let batch_retry_attempts = if options.get_retry_attempts() >= 0 {
            options.get_retry_attempts() as u32
        } else {
            cfg.retry_attempts
        };
        let batch_retry_delay = options.get_retry_delay().unwrap_or(cfg.retry_delay);

        let initial_state = if options.is_redis_based_queue() {
            BatchState::RedisBased { slot: None, entries: Vec::new() }
        } else {
            BatchState::InMemory(Vec::new())
        };

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
            executed:   AtomicBool::new(false),
            next_index: AtomicUsize::new(0),
            state:      tokio::sync::Mutex::new(initial_state),
        }
    }

    /// 使用默认 BatchOptions 构造。
    pub fn new_default(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self::new(connection_manager, BatchOptions::defaults())
    }

    pub fn get_options(&self) -> &BatchOptions { &self.options }
    pub fn is_redis_based_queue(&self) -> bool { self.options.is_redis_based_queue() }
    pub fn is_executed(&self) -> bool { self.executed.load(Ordering::Acquire) }

    /// 对应 Java CommandBatchService.executeAsyncVoid()。
    pub async fn execute_async_void(&self) -> anyhow::Result<()> {
        self.execute_async().await?;
        Ok(())
    }

    /// 对应 Java CommandBatchService.discardAsync()。
    pub async fn discard_async(&self) -> anyhow::Result<()> {
        if self.executed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            anyhow::bail!("Batch already executed!");
        }
        let err = || anyhow::anyhow!("batch discarded");
        let mut st = self.state.lock().await;

        match &mut *st {
            BatchState::InMemory(vec) => {
                for data in std::mem::take(vec) {
                    let _ = data.tx.send(Err(err()));
                }
            }
            BatchState::RedisBased { entries, .. } => {
                for (tx, _) in std::mem::take(entries) {
                    let _ = tx.send(Err(err()));
                }
            }
        }
        Ok(())
    }

    // ── execute_async ────────────────────────────────────────────

    /// 对应 Java CommandBatchService.executeAsync()。
    pub async fn execute_async(&self) -> anyhow::Result<BatchResult> {
        if self.executed.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            anyhow::bail!("Batch already executed!");
        }

        match self.options.get_execution_mode() {
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic => {
                self.execute_redis_based_queue().await
            }
            mode => {
                self.execute_in_memory(*mode == ExecutionMode::InMemoryAtomic).await
            }
        }
    }

    // ── REDIS_*_ATOMIC 路径 ──────────────────────────────────────

    /// 对应 Java CommandBatchService.executeRedisBasedQueue()。
    async fn execute_redis_based_queue(&self) -> anyhow::Result<BatchResult> {
        let timeout = self.options.get_response_timeout()
            .unwrap_or(self.inner.connection_manager.config().command_timeout);
        tokio::time::timeout(timeout, self.execute_redis_based_queue_inner())
            .await
            .map_err(|_| anyhow::anyhow!("Batch execution timed out after {:?}", timeout))?
    }

    async fn execute_redis_based_queue_inner(&self) -> anyhow::Result<BatchResult> {
        let pool = &self.inner.connection_manager.pool;
        let (entries, slot) = {
            let mut st = self.state.lock().await;
            let BatchState::RedisBased { slot, entries } = &mut *st else {
                anyhow::bail!("unexpected batch state in execute_redis_based_queue");
            };
            (std::mem::take(entries), slot.unwrap_or(0))
        };

        if entries.is_empty() {
            return Ok(BatchResult::new(Vec::new(), 0));
        }

        let is_read         = matches!(self.options.get_execution_mode(), ExecutionMode::RedisReadAtomic);
        let is_skip         = self.options.is_skip_result();
        let sync_slaves_opt = self.options.get_sync_slaves();
        let sync_aof        = self.options.is_sync_aof();
        let sync_locals     = self.options.get_sync_locals();
        let sync_timeout    = self.options.get_sync_timeout().as_millis() as i64;

        let (txs, indexes): (Vec<_>, Vec<_>) = entries.into_iter().unzip();

        // 所有命令已在 async0 时以 MULTI 状态发到目标节点，这里只需发 EXEC
        let exec_cmd = CustomCommand::new_static("EXEC", ClusterHash::Custom(slot), false);
        let exec_result: Result<Value, _> = if is_read {
            pool.replicas().custom(exec_cmd, Vec::<Value>::new()).await
        } else {
            pool.next().custom(exec_cmd, Vec::<Value>::new()).await
        };

        let mut synced_slaves: i64 = 0;
        if sync_slaves_opt > 0 {
            let (wait_cmd, wait_args) = Self::make_wait_cmd(slot, sync_aof, sync_locals, sync_slaves_opt, sync_timeout);
            let wait_result: Result<Value, _> = if is_read {
                pool.replicas().custom(wait_cmd, wait_args).await
            } else {
                pool.next().custom(wait_cmd, wait_args).await
            };
            synced_slaves = Self::extract_synced_slaves(Some(wait_result));
        }

        let mut all_indexed: Vec<(usize, Value)> = Vec::new();
        if is_skip {
            for tx in txs { let _ = tx.send(Ok(Value::Null)); }
        } else {
            match exec_result {
                Ok(Value::Array(actual)) => {
                    for ((tx, idx), val) in txs.into_iter().zip(indexes.into_iter()).zip(actual.into_iter()) {
                        let _ = tx.send(Ok(val.clone()));
                        all_indexed.push((idx, val));
                    }
                }
                Ok(Value::Null) => {
                    for tx in txs {
                        let _ = tx.send(Err(anyhow::anyhow!("EXEC returned nil: transaction aborted by WATCH")));
                    }
                }
                Ok(other) => anyhow::bail!("unexpected EXEC response: {:?}", other),
                Err(e) => {
                    for tx in txs { let _ = tx.send(Err(anyhow::anyhow!("{e}"))); }
                }
            }
        }

        all_indexed.sort_by_key(|(i, _)| *i);
        Ok(BatchResult::new(all_indexed.into_iter().map(|(_, v)| v).collect(), synced_slaves))
    }

    // ── IN_MEMORY / IN_MEMORY_ATOMIC 路径 ────────────────────────

    async fn execute_in_memory(&self, is_atomic: bool) -> anyhow::Result<BatchResult> {
        let pool = &self.inner.connection_manager.pool;

        let entries = {
            let mut st = self.state.lock().await;
            let BatchState::InMemory(vec) = &mut *st else {
                anyhow::bail!("unexpected batch state in execute_in_memory");
            };
            std::mem::take(vec)
        };

        if entries.is_empty() {
            return Ok(BatchResult::new(Vec::new(), 0));
        }

        let is_skip         = self.options.is_skip_result();
        let sync_slaves_opt = self.options.get_sync_slaves();
        let sync_aof        = self.options.is_sync_aof();
        let sync_locals     = self.options.get_sync_locals();
        let sync_timeout    = self.options.get_sync_timeout().as_millis() as i64;

        let mut node_map: HashMap<MasterSlaveEntry, Entry> = HashMap::new();
        for data in entries {
            let slot = data.cmd.routing_key().map(redis_keyslot).unwrap_or(0);
            let mse  = self.resolve_entry(slot).await?;
            node_map.entry(mse).or_insert_with(Entry::new).add_command(data);
        }
        self.load_scripts(&mut node_map).await?;

        let pool_owned = pool.clone();
        let read_mode  = self.inner.connection_manager.read_mode.clone();
        let mut join_set: tokio::task::JoinSet<anyhow::Result<(i64, Vec<(usize, Value)>)>> =
            tokio::task::JoinSet::new();

        for (mse, entry) in node_map {
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

                let pipeline = if use_replica {
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

        let mut synced_slaves: i64 = 0;
        let mut all_indexed: Vec<(usize, Value)> = Vec::new();
        while let Some(res) = join_set.join_next().await {
            let (s, idx) = res.map_err(|e| anyhow::anyhow!("batch task panicked: {e}"))??;
            synced_slaves += s;
            all_indexed.extend(idx);
        }

        all_indexed.sort_by_key(|(i, _)| *i);
        Ok(BatchResult::new(all_indexed.into_iter().map(|(_, v)| v).collect(), synced_slaves))
    }

    // ── 辅助：节点解析（带重试）──────────────────────────────────

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

    // ── 辅助：EXEC 结果分发 ───────────────────────────────────────

    fn distribute_exec_results(
        exec_result: Option<Result<Value, fred::error::Error>>,
        txs: Vec<oneshot::Sender<anyhow::Result<Value>>>,
    ) -> anyhow::Result<()> {
        match exec_result {
            None => return Err(anyhow::anyhow!("empty pipeline result, EXEC response missing")),
            Some(Ok(Value::Array(actual))) => {
                for (tx, val) in txs.into_iter().zip(actual.into_iter()) {
                    let _ = tx.send(Ok(val));
                }
            }
            Some(Ok(Value::Null)) => {
                for tx in txs {
                    let _ = tx.send(Err(anyhow::anyhow!("EXEC returned nil: transaction aborted by WATCH")));
                }
            }
            Some(Ok(other)) => {
                return Err(anyhow::anyhow!("unexpected EXEC response format: {:?}", other));
            }
            Some(Err(e)) => {
                for tx in txs {
                    let _ = tx.send(Err(anyhow::anyhow!("{}", e)));
                }
            }
        }
        Ok(())
    }

    // ── 辅助：提取 syncedSlaves ───────────────────────────────────

    fn extract_synced_slaves(wait_result: Option<Result<Value, fred::error::Error>>) -> i64 {
        match wait_result {
            Some(Ok(Value::Integer(n))) => n,
            Some(Ok(Value::Array(nums))) => {
                nums.get(1).and_then(|v| match v {
                    Value::Integer(n) => Some(*n),
                    _ => None,
                }).unwrap_or(0)
            }
            _ => 0,
        }
    }

    // ── 辅助：EXEC 结果收集带 index ──────────────────────────────

    fn collect_exec_indexed(
        exec_result: Option<Result<Value, fred::error::Error>>,
        indexes: Vec<usize>,
        all_indexed: &mut Vec<(usize, Value)>,
    ) {
        if let Some(Ok(Value::Array(actual))) = exec_result {
            for (idx, val) in indexes.into_iter().zip(actual.into_iter()) {
                all_indexed.push((idx, val));
            }
        }
    }

    // ── 对应 Java CommandBatchService.loadScripts() ───────────────

    async fn load_scripts(
        &self,
        result: &mut HashMap<MasterSlaveEntry, Entry>,
    ) -> anyhow::Result<()> {
        if !self.inner.connection_manager.config().use_script_cache {
            return Ok(());
        }
        let pool = &self.inner.connection_manager.pool;

        for (mse, entry) in result.iter_mut() {
            let slot   = mse.slot().unwrap_or(0);
            let server = mse.get_client();

            let mut to_load: Vec<String> = Vec::new();
            for data in entry.commands.iter() {
                if let RedisCommand::Eval { script, .. } = &data.cmd {
                    if !ServiceManager::is_cached(server, script) && !to_load.contains(script) {
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
                        .map_err(|e| anyhow::anyhow!("SCRIPT LOAD failed: {e}"))?;
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
                if let Some(nc) = new_cmd { data.cmd = nc; }
            }
        }
        Ok(())
    }

    // ── 辅助：构造 WAIT / WAITAOF 命令 ───────────────────────────

    fn make_wait_cmd(
        slot:        u16,
        sync_aof:    bool,
        sync_locals: i64,
        sync_slaves: i64,
        timeout_ms:  i64,
    ) -> (CustomCommand, Vec<Value>) {
        if sync_aof {
            (
                CustomCommand::new_static("WAITAOF", ClusterHash::Custom(slot), false),
                vec![Value::Integer(sync_locals), Value::Integer(sync_slaves), Value::Integer(timeout_ms)],
            )
        } else {
            (
                CustomCommand::new_static("WAIT", ClusterHash::Custom(slot), false),
                vec![Value::Integer(sync_slaves), Value::Integer(timeout_ms)],
            )
        }
    }

    fn is_wait_command(cmd: &RedisCommand) -> bool {
        matches!(cmd, RedisCommand::Wait { .. } | RedisCommand::WaitAof { .. })
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
    async fn async0(&self, cmd: RedisCommand) -> anyhow::Result<Value> {
        let (tx, rx) = oneshot::channel();
        let idx = self.next_index.fetch_add(1, Ordering::Relaxed);

        let mut st = self.state.lock().await;
        match &mut *st {
            BatchState::InMemory(vec) => {
                vec.push(BatchCommandData { cmd, tx, index: idx });
            }
            BatchState::RedisBased { slot, entries } => {
                if Self::is_wait_command(&cmd) {
                    let _ = tx.send(Err(anyhow::anyhow!(
                        "WAIT/WAITAOF is not supported in REDIS_*_ATOMIC mode"
                    )));
                    drop(st);
                    return rx.await.map_err(|_| anyhow::anyhow!("batch channel closed"))?;
                }

                let pool = &self.inner.connection_manager.pool;
                let is_read = matches!(self.options.get_execution_mode(), ExecutionMode::RedisReadAtomic);

                // 第一条命令：确定 slot，发 MULTI 到目标节点
                if entries.is_empty() {
                    if let Some(key_bytes) = cmd.routing_key() {
                        *slot = Some(redis_keyslot(key_bytes));
                    }
                    let current_slot = slot.unwrap_or(0);
                    let multi_cmd = CustomCommand::new_static("MULTI", ClusterHash::Custom(current_slot), false);
                    let _: Value = if is_read {
                        pool.replicas().custom(multi_cmd, Vec::<Value>::new()).await
                    } else {
                        pool.next().custom(multi_cmd, Vec::<Value>::new()).await
                    }.map_err(|e| anyhow::anyhow!("MULTI failed: {e}"))?;
                }

                // 立即发往 Redis（同一 slot 保证路由到同一节点，Redis 返回 QUEUED）
                if is_read {
                    cmd.execute_on(&pool.replicas()).await?;
                } else {
                    cmd.execute_on(pool.next()).await?;
                }
                entries.push((tx, idx));
            }
        }
        drop(st);

        rx.await.map_err(|_| anyhow::anyhow!("batch channel closed (execute_async not called before drop)"))?
    }

    fn is_eval_cache_active(&self) -> bool { false }
    fn is_batch(&self) -> bool { true }
}
