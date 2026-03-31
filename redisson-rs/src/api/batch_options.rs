use crate::config::equal_jitter_delay::DelayStrategy;
use std::sync::Arc;
use std::time::Duration;

// ============================================================
// ExecutionMode — 对应 Java BatchOptions.ExecutionMode 枚举
// ============================================================

/// 对应 Java org.redisson.api.BatchOptions.ExecutionMode。
/// 控制 batch 命令的执行方式。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ExecutionMode {
    /// 内存中聚合后统一发送（默认）。对应 Java IN_MEMORY。
    #[default]
    InMemory,
    /// 内存聚合 + MULTI/EXEC 原子执行。对应 Java IN_MEMORY_ATOMIC。
    InMemoryAtomic,
    /// 命令通过 MULTI/EXEC 逐条入队到 Redis（写）后统一 EXEC。对应 Java REDIS_WRITE_ATOMIC。
    RedisWriteAtomic,
    /// 命令通过 MULTI/EXEC 逐条入队到 Redis（读）后统一 EXEC。对应 Java REDIS_READ_ATOMIC。
    RedisReadAtomic,
}

// ============================================================
// BatchOptions — 对应 Java org.redisson.api.BatchOptions（类）
// ============================================================

/// 对应 Java org.redisson.api.BatchOptions（final 类）。
/// 控制 RBatch 的执行模式、超时、重试、从节点同步等行为。
#[derive(Clone)]
pub struct BatchOptions {
    /// 对应 Java BatchOptions.executionMode（默认 IN_MEMORY）
    pub execution_mode: ExecutionMode,
    /// 对应 Java BatchOptions.responseTimeout（毫秒，0 表示使用全局配置）
    pub response_timeout: u64,
    /// 对应 Java BatchOptions.retryAttempts（-1 表示使用全局配置）
    pub retry_attempts: i32,
    /// 对应 Java BatchOptions.retryDelay（None 表示使用全局配置）
    pub retry_delay: Option<Arc<dyn DelayStrategy + Send + Sync>>,
    /// 对应 Java BatchOptions.syncTimeout（从节点同步超时，毫秒）
    pub sync_timeout: u64,
    /// 对应 Java BatchOptions.syncSlaves（需同步的从节点数）
    pub sync_slaves: i32,
    /// 对应 Java BatchOptions.syncLocals（需同步的本地节点数，AOF 模式）
    pub sync_locals: i32,
    /// 对应 Java BatchOptions.syncAOF（是否等待 AOF 持久化）
    pub sync_aof: bool,
    /// 对应 Java BatchOptions.skipResult（不等待响应，适合写入不关心结果场景）
    pub skip_result: bool,
}

impl std::fmt::Debug for BatchOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchOptions")
            .field("execution_mode", &self.execution_mode)
            .field("response_timeout", &self.response_timeout)
            .field("retry_attempts", &self.retry_attempts)
            .field("retry_delay", &self.retry_delay.as_ref().map(|_| "<DelayStrategy>"))
            .field("sync_timeout", &self.sync_timeout)
            .field("sync_slaves", &self.sync_slaves)
            .field("sync_locals", &self.sync_locals)
            .field("sync_aof", &self.sync_aof)
            .field("skip_result", &self.skip_result)
            .finish()
    }
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            execution_mode: ExecutionMode::InMemory,
            response_timeout: 0,
            retry_attempts: -1,
            retry_delay: None,
            sync_timeout: 0,
            sync_slaves: 0,
            sync_locals: 0,
            sync_aof: false,
            skip_result: false,
        }
    }
}

impl BatchOptions {
    /// 对应 Java BatchOptions.defaults()
    pub fn defaults() -> Self {
        Self::default()
    }

    /// 对应 Java BatchOptions.executionMode(ExecutionMode)
    pub fn execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

    /// 对应 Java BatchOptions.responseTimeout(long, TimeUnit)
    pub fn response_timeout(mut self, timeout: Duration) -> Self {
        self.response_timeout = timeout.as_millis() as u64;
        self
    }

    /// 对应 Java BatchOptions.retryAttempts(int)
    pub fn retry_attempts(mut self, attempts: i32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    /// 对应 Java BatchOptions.retryDelay(DelayStrategy)
    pub fn retry_delay(mut self, delay: Arc<dyn DelayStrategy + Send + Sync>) -> Self {
        self.retry_delay = Some(delay);
        self
    }

    /// 对应 Java BatchOptions.sync(int slaves, Duration timeout)
    /// 等待指定数量从节点同步后才返回结果。
    pub fn sync(mut self, slaves: i32, timeout: Duration) -> Self {
        self.sync_slaves = slaves;
        self.sync_timeout = timeout.as_millis() as u64;
        self
    }

    /// 对应 Java BatchOptions.syncAOF(int localNum, int slaves, Duration timeout)
    /// 等待 AOF 持久化 + 从节点同步后才返回结果。
    pub fn sync_aof(mut self, local_num: i32, slaves: i32, timeout: Duration) -> Self {
        self.sync_locals = local_num;
        self.sync_slaves = slaves;
        self.sync_timeout = timeout.as_millis() as u64;
        self.sync_aof = true;
        self
    }

    /// 对应 Java BatchOptions.skipResult()
    /// 不等待服务端响应，适合写入不关心结果的场景。
    pub fn skip_result(mut self) -> Self {
        self.skip_result = true;
        self
    }

    /// 对应 Java BatchOptions.isSkipResult()
    pub fn is_skip_result(&self) -> bool {
        self.skip_result
    }

    /// 对应 Java BatchOptions.isRedisBasedQueue()（在 CommandBatchService 中判断）
    pub fn is_redis_based_queue(&self) -> bool {
        matches!(
            self.execution_mode,
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic
        )
    }
}
