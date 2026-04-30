use crate::api::batch_options::BatchOptions;
use crate::api::sync_mode::SyncMode;
use crate::config::ServerMode;
use crate::client::protocol::redis_command::RedisCommand;
use crate::command::command_batch_service::CommandBatchService;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::{CachedSlaveInfo, ServiceManager};
use fred::clients::Pool;
use fred::interfaces::{ClientLike, LuaInterface, ServerInterface};
use fred::types::{ClusterHash, CustomCommand, InfoKind, Key, Value};
use fred::types::config::{Options, Server};
use fred::util::redis_keyslot;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// 对应 Java CommandAsyncService.SORT_RO_SUPPORTED。
pub(crate) static SORT_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);

// 对应 Java CommandAsyncService.EVAL_SHA_RO_SUPPORTED。
pub(crate) static EVAL_SHA_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);

pub(crate) trait CommandAsyncServiceLike: Send + Sync {
    fn inner(&self) -> &CommandAsyncInner;

    fn is_eval_cache_active(&self) -> bool {
        self.inner().connection_manager.config().use_script_cache
    }

    fn is_batch(&self) -> bool {
        false
    }

    /// 对应 Java CommandAsyncService.async()
    async fn async0(&self, cmd: RedisCommand) -> anyhow::Result<Value> {
        let inner = self.inner();
        let pool = &inner.connection_manager.pool;
        let options = inner.build_options();
        let is_write = !cmd.is_read_only();
        let key = cmd.routing_key()
            .and_then(|k| std::str::from_utf8(k).ok())
            .map(|s| s.to_owned());
        let result = cmd.execute(pool, &options).await?;
        if is_write {
            let sm = inner.connection_manager.service_manager();
            if sm.has_caching_instances() {
                if let Some(name) = key {
                    sm.evict_client_side_caching(&name);
                }
            }
        }
        Ok(result)
    }

    /// 对应 Java CommandAsyncService.writeAsync(String key, ...)
    async fn write_async(&self, cmd: RedisCommand) -> anyhow::Result<Value> {
        self.async0(cmd).await
    }

    /// 对应 Java CommandAsyncService.evalWriteAsync(String key, ...)
    async fn eval_write_async(
        &self,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        self.eval_async(false, false, script, keys, args).await
    }

    /// 对应 Java CommandAsyncService.evalWriteNoRetryAsync(String key, ...)
    async fn eval_write_no_retry_async(
        &self,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        self.eval_async(false, true, script, keys, args).await
    }

    /// 对应 Java CommandAsyncService.evalReadAsync(String key, ...)
    async fn eval_read_async(
        &self,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        self.eval_async(true, false, script, keys, args).await
    }

    /// 对应 Java CommandAsyncService.evalAsync()
    /// isEvalCacheActive=true 时先走 EVALSHA（或 EVALSHA_RO），失败时：
    ///   - ERR unknown command → EVALSHA_RO_SUPPORTED 置 false，递归重试（此时会走 EVALSHA 分支）
    ///   - NOSCRIPT → SCRIPT LOAD 广播所有主节点后，按 read_only 重试 EVALSHA/EVALSHA_RO
    /// isEvalCacheActive=false 时直接走 EVAL。
    async fn eval_async(
        &self,
        read_only: bool,
        no_retry: bool,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        let inner = self.inner();
        let pool = &inner.connection_manager.pool;
        let options = if no_retry {
            Options { max_attempts: Some(1), ..inner.build_options() }
        } else {
            inner.build_options()
        };

        if !self.is_eval_cache_active() {
            return RedisCommand::Eval { script, keys, args }
                .execute(pool, &options)
                .await;
        }

        let sha = ServiceManager::calc_sha(&script);
        let cmd = if read_only && EVAL_SHA_RO_SUPPORTED.load(Ordering::Relaxed) {
            RedisCommand::EvalShaRo { sha: sha.clone(), keys: keys.clone(), args: args.clone() }
        } else {
            RedisCommand::EvalSha { sha: sha.clone(), keys: keys.clone(), args: args.clone() }
        };

        match cmd.execute(pool, &options).await {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("ERR unknown command") {
                    // EVALSHA_RO 不被支持，标记后让下面同一套 flag 决策重试，
                    // 此时 EVAL_SHA_RO_SUPPORTED=false，必然选 EvalSha。
                    EVAL_SHA_RO_SUPPORTED.store(false, Ordering::Relaxed);
                    let retry_cmd = if read_only && EVAL_SHA_RO_SUPPORTED.load(Ordering::Relaxed) {
                        RedisCommand::EvalShaRo { sha, keys, args }
                    } else {
                        RedisCommand::EvalSha { sha, keys, args }
                    };
                    retry_cmd.execute(pool, &options).await
                } else if msg.contains("NOSCRIPT") {
                    // cluster 模式下广播到所有主节点，避免重试路由到未加载脚本的节点；
                    // 非 cluster 模式退化为普通 SCRIPT LOAD。
                    let _: Value = pool.script_load_cluster(script.as_str()).await
                        .map_err(anyhow::Error::from)?;
                    let retry_cmd = if read_only && EVAL_SHA_RO_SUPPORTED.load(Ordering::Relaxed) {
                        RedisCommand::EvalShaRo { sha, keys, args }
                    } else {
                        RedisCommand::EvalSha { sha, keys, args }
                    };
                    retry_cmd.execute(pool, &options).await
                } else {
                    Err(e)
                }
            }
        }
    }

    /// 对应 Java CommandAsyncService.syncedEvalWithRetry()
    async fn synced_eval_with_retry(
        &self,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        self.synced_eval(SyncMode::Wait, true, script, keys, args).await
    }

    /// 对应 Java CommandAsyncService.syncedEvalNoRetry()
    async fn synced_eval_no_retry(
        &self,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        self.synced_eval(SyncMode::Wait, false, script, keys, args).await
    }

    /// 对应 Java CommandAsyncService.syncedEval(long timeout, SyncMode, boolean retry, ...)
    async fn synced_eval(
        &self,
        sync_mode: SyncMode,
        retry: bool,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        let inner = self.inner();
        let pool = &inner.connection_manager.pool;
        let cfg = inner.connection_manager.config();

        // 单节点不需要 WAIT，CommandBatchService 内也不套娃。
        let is_single = matches!(cfg.mode, ServerMode::Standalone { .. });
        if is_single || self.is_batch() {
            return self.eval_write_fallback(retry, script, keys, args).await;
        }

        // INFO 和 WAIT 必须路由到持有该 key 的主节点，而非连接池随机节点，
        // 否则 cluster 模式下查的从库数可能属于其他分片。
        let slot = keys.first()
            .map(|k| redis_keyslot(k.as_bytes()))
            .unwrap_or(0);
        let cluster_node: Option<Server> = inner.connection_manager
            .get_write_entry(slot)
            .map(|e| e.primary.clone());
        let node_opts = Options { cluster_node: cluster_node.clone(), ..inner.build_options() };

        // 首次调用时向目标节点发 WAIT 0 0 和 WAITAOF 0 0 0（立即返回），
        // 根据是否报 ERR unknown command 判断支持情况，结果缓存后不再重探。
        let wait_support = {
            let mut guard = inner.wait_support.lock().await;
            if guard.is_none() {
                *guard = Some(probe_wait_support(pool, &node_opts).await?);
            }
            guard.clone().unwrap()
        };

        // SyncMode 决定的跳过条件，对应 Java syncedEval 开头的 if 判断。
        let skip = match sync_mode {
            SyncMode::Auto    => !wait_support.wait && !wait_support.wait_aof,
            SyncMode::Wait    => !wait_support.wait,
            SyncMode::WaitAof => !wait_support.wait_aof,
        };
        if skip {
            return self.eval_write_fallback(retry, script, keys, args).await;
        }

        // 对应 Java e.getAvailableSlaves()：缓存命中则跳过 INFO 查询；
        // 缓存缺失（None）等价于 Java 的 availableSlaves == -1。
        let repl = match cluster_node.as_ref().and_then(ServiceManager::get_slave_info) {
            Some(cached) => ReplicationInfo {
                connected_slaves: cached.connected_slaves,
                aof_enabled:      cached.aof_enabled,
            },
            None => {
                let info = query_replication_info(pool, &node_opts).await?;
                if let Some(server) = &cluster_node {
                    ServiceManager::set_slave_info(server.clone(), CachedSlaveInfo {
                        connected_slaves: info.connected_slaves,
                        aof_enabled:      info.aof_enabled,
                    });
                }
                info
            }
        };

        // WAITAOF 只在 Auto / WaitAof 模式下且节点确实启用了 AOF 时使用。
        let use_aof = wait_support.wait_aof
            && repl.aof_enabled
            && matches!(sync_mode, SyncMode::Auto | SyncMode::WaitAof);
        let available_slaves: i64 = if wait_support.wait { repl.connected_slaves } else { 0 };

        if available_slaves == 0 && !use_aof {
            return self.eval_write_fallback(retry, script, keys, args).await;
        }

        let sync_timeout = cfg.slaves_sync_timeout;

        let mut script_loaded = false;
        loop {
            if script_loaded {
                // 脚本不在该节点，广播加载后重试；只重试一次。
                let _: Value = pool.script_load_cluster(script.as_str()).await
                    .map_err(anyhow::Error::from)?;
            }

            let batch_opts = if use_aof {
                BatchOptions::defaults().sync_aof(1, available_slaves, sync_timeout)
            } else {
                BatchOptions::defaults().sync(available_slaves, sync_timeout)
            };
            let batch = Arc::new(CommandBatchService::new(
                inner.connection_manager.clone(),
                batch_opts,
            ));

            // async0 在 InMemory 模式下会阻塞等 execute_async 发来结果，
            // 必须并发执行：spawn 入队并等结果，主线程负责 execute_async。
            let batch_clone = batch.clone();
            let (s, k, a) = (script.clone(), keys.clone(), args.clone());
            let eval_handle = tokio::spawn(async move {
                batch_clone.async0(RedisCommand::Eval { script: s, keys: k, args: a }).await
            });

            let batch_result = match batch.execute_async().await {
                Ok(r) => r,
                Err(e) => {
                    eval_handle.abort();
                    if !script_loaded && e.to_string().contains("NOSCRIPT") {
                        script_loaded = true;
                        continue;
                    }
                    return Err(e);
                }
            };

            // 对应 Java e.setAvailableSlaves(-1)：实际同步数与预期不符说明拓扑已变，
            // 缓存失效，下次 synced_eval 重新查 INFO。
            if batch_result.synced_slaves != available_slaves {
                if let Some(server) = &cluster_node {
                    ServiceManager::invalidate_slave_info(server);
                }
            }

            if cfg.check_lock_synced_slaves
                && batch_result.synced_slaves == 0
                && available_slaves > 0
            {
                return Err(anyhow::anyhow!(
                    "None of slaves were synced. \
                     Try to increase slavesSyncTimeout or set checkLockSyncedSlaves = false."
                ));
            }

            return eval_handle.await
                .map_err(|e| anyhow::anyhow!("synced_eval task panicked: {e}"))?;
        }
    }

    /// 对应 Java syncedEval 跳过条件触发后，根据 retry 选择有无重试的 eval。
    async fn eval_write_fallback(
        &self,
        retry: bool,
        script: String,
        keys: Vec<Key>,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        if retry {
            self.eval_write_async(script, keys, args).await
        } else {
            self.eval_write_no_retry_async(script, keys, args).await
        }
    }
}

pub struct CommandAsyncInner {
    pub connection_manager: Arc<FredConnectionManager>,
    pub retry_attempts: Option<u32>,
    pub response_timeout: Option<Duration>,
    pub track_changes: bool,
    /// WAIT/WAITAOF 支持情况（首次 synced_eval 时惰性探测，结果缓存后不再重复探测）。
    wait_support: Arc<tokio::sync::Mutex<Option<WaitSupport>>>,
}

impl CommandAsyncInner {
    pub fn new(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self::new_all_params(connection_manager, None, None, false)
    }

    pub fn new_all_params(
        connection_manager: Arc<FredConnectionManager>,
        retry_attempts: Option<u32>,
        response_timeout: Option<Duration>,
        track_changes: bool,
    ) -> Self {
        Self {
            connection_manager,
            retry_attempts,
            response_timeout,
            track_changes,
            wait_support: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub(crate) fn build_options(&self) -> Options {
        Options {
            max_attempts: self.retry_attempts,
            timeout:      self.response_timeout,
            ..Default::default()
        }
    }
}

pub struct CommandAsyncService {
    pub(crate) inner: CommandAsyncInner,
}

impl CommandAsyncService {
    pub fn new(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self {
            inner: CommandAsyncInner::new(connection_manager),
        }
    }
}

impl CommandAsyncServiceLike for CommandAsyncService {
    fn inner(&self) -> &CommandAsyncInner {
        &self.inner
    }
}

// ── WAIT/WAITAOF 支持状态 ────────────────────────────────────────────────────

/// 对应 Java CommandAsyncService.waitSupportedCommands。
#[derive(Debug, Clone)]
struct WaitSupport {
    /// Redis 节点支持 WAIT 命令
    wait:     bool,
    /// Redis 节点支持 WAITAOF 命令（Redis 7.2+）
    wait_aof: bool,
}

// ── 复制信息 ─────────────────────────────────────────────────────────────────

struct ReplicationInfo {
    connected_slaves: i64,
    aof_enabled:      bool,
}

// ── 辅助：探测单条命令是否被支持 ──────────────────────────────────────────────

/// `Ok(true)` = 命令存在，`Ok(false)` = ERR unknown command，`Err` = 其他真实错误。
async fn probe_command(
    pool: &Pool,
    opts: &Options,
    cmd: &'static str,
    args: Vec<Value>,
) -> anyhow::Result<bool> {
    match pool
        .with_options(opts)
        .custom::<Value, _>(CustomCommand::new_static(cmd, ClusterHash::FirstKey, false), args)
        .await
    {
        Ok(_) => Ok(true),
        Err(e) if e.details().contains("ERR unknown command") => Ok(false),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

/// 对应 Java syncedEval 里的 WAIT/WAITAOF 探测 batch。
/// 向节点发送 WAIT 0 0 和 WAITAOF 0 0 0（立即返回），判断哪些命令受支持。
/// 真实错误（非 ERR unknown command）直接向上传播。
async fn probe_wait_support(pool: &Pool, opts: &Options) -> anyhow::Result<WaitSupport> {
    let wait = probe_command(pool, opts, "WAIT",
        vec![Value::Integer(0), Value::Integer(0)]).await?;
    let wait_aof = probe_command(pool, opts, "WAITAOF",
        vec![Value::Integer(0), Value::Integer(0), Value::Integer(0)]).await?;
    Ok(WaitSupport { wait, wait_aof })
}

/// INFO all 查询指定节点的复制及 AOF 状态。
async fn query_replication_info(pool: &Pool, opts: &Options) -> anyhow::Result<ReplicationInfo> {
    let info: String = pool
        .with_options(opts)
        .info(Some(InfoKind::All))
        .await
        .map_err(anyhow::Error::from)?;
    let connected_slaves = parse_info_i64(&info, "connected_slaves");
    let aof_enabled      = parse_info_str(&info, "aof_enabled") == Some("1");
    Ok(ReplicationInfo { connected_slaves, aof_enabled })
}

fn parse_info_i64(info: &str, key: &str) -> i64 {
    parse_info_str(info, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn parse_info_str<'a>(info: &'a str, key: &str) -> Option<&'a str> {
    for line in info.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(rest) = rest.strip_prefix(':') {
                return Some(rest.trim());
            }
        }
    }
    None
}
