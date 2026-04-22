// 对应 Java org.redisson.api.BatchOptions
//
// Java 的 BatchOptions 是一个 builder 风格的配置类，携带 batch 执行所需的全部参数：
// 执行模式、同步从节点、AOF 同步、超时、重试等。
// Rust 侧用同名结构体对齐，builder 方法返回 Self（消费 self）保持链式调用风格。

use std::time::Duration;

// ============================================================
// ExecutionMode — 对应 Java BatchOptions.ExecutionMode
// ============================================================

/// 对应 Java BatchOptions.ExecutionMode 枚举。
/// 控制 batch 命令在 Redisson 侧和 Redis 侧如何缓冲和执行。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// 对应 Java ExecutionMode.IN_MEMORY（默认）。
    /// 命令在 Redisson 内存中缓冲，执行时以 pipeline 批量发往 Redis。
    #[default]
    InMemory,

    /// 对应 Java ExecutionMode.IN_MEMORY_ATOMIC。
    /// 命令在 Redisson 内存中缓冲，执行时以 MULTI/EXEC 原子性发往 Redis。
    InMemoryAtomic,

    /// 对应 Java ExecutionMode.REDIS_READ_ATOMIC。
    /// 命令逐条存入 Redis 列表队列（只读），最终原子执行。
    RedisReadAtomic,

    /// 对应 Java ExecutionMode.REDIS_WRITE_ATOMIC。
    /// 命令逐条存入 Redis 列表队列（读写），最终原子执行。
    RedisWriteAtomic,
}

// ============================================================
// BatchOptions — 对应 Java BatchOptions
// ============================================================

/// 对应 Java org.redisson.api.BatchOptions。
/// 使用 builder 模式配置 RBatch 的执行行为。
///
/// 示例：
/// ```rust
/// use std::time::Duration;
/// use redisson_rs::api::batch_options::BatchOptions;
///
/// let opts = BatchOptions::defaults()
///     .sync(1, Duration::from_secs(5));
/// ```
#[derive(Debug, Clone)]
pub struct BatchOptions {
    /// 对应 Java BatchOptions.executionMode
    pub(crate) execution_mode: ExecutionMode,

    /// 对应 Java BatchOptions.responseTimeout（None 表示使用全局配置）
    pub(crate) response_timeout: Option<Duration>,

    /// 对应 Java BatchOptions.retryAttempts（-1 表示使用全局配置）
    pub(crate) retry_attempts: i32,

    // TODO: retryDelay (DelayStrategy) — 待 DelayStrategy trait 稳定后添加

    /// 对应 Java BatchOptions.syncSlaves
    /// WAIT 命令等待的从节点数量
    pub(crate) sync_slaves: u32,

    /// 对应 Java BatchOptions.syncLocals
    /// WAITAOF 命令等待的本地 Redis 数量
    pub(crate) sync_locals: u32,

    /// 对应 Java BatchOptions.syncTimeout
    /// WAIT / WAITAOF 的等待超时
    pub(crate) sync_timeout: Duration,

    /// 对应 Java BatchOptions.syncAOF
    /// true 时使用 WAITAOF，false 时使用 WAIT
    pub(crate) sync_aof: bool,

    /// 对应 Java BatchOptions.skipResult
    /// true 时跳过响应结果（节省网络流量，适合 fire-and-forget 场景）
    pub(crate) skip_result: bool,
}

impl BatchOptions {
    // --------------------------------------------------------
    // 工厂方法
    // --------------------------------------------------------

    /// 对应 Java BatchOptions.defaults()。
    /// 返回默认配置：IN_MEMORY 模式，不同步从节点，不跳过结果。
    pub fn defaults() -> Self {
        Self {
            execution_mode: ExecutionMode::InMemory,
            response_timeout: None,
            retry_attempts: -1,
            sync_slaves: 0,
            sync_locals: 0,
            sync_timeout: Duration::ZERO,
            sync_aof: false,
            skip_result: false,
        }
    }

    // --------------------------------------------------------
    // Builder 方法（消费 self，支持链式调用）
    // --------------------------------------------------------

    /// 对应 Java BatchOptions.sync(int slaves, Duration timeout)。
    /// 执行后等待指定数量的从节点同步写操作（通过 WAIT 命令）。
    pub fn sync(mut self, slaves: u32, timeout: Duration) -> Self {
        self.sync_slaves = slaves;
        self.sync_timeout = timeout;
        self.sync_aof = false;
        self
    }

    /// 对应 Java BatchOptions.syncAOF(int localNum, int slaves, Duration timeout)。
    /// 执行后等待 AOF 持久化完成（通过 WAITAOF 命令）。
    pub fn sync_aof(mut self, local_num: u32, slaves: u32, timeout: Duration) -> Self {
        self.sync_locals = local_num;
        self.sync_slaves = slaves;
        self.sync_timeout = timeout;
        self.sync_aof = true;
        self
    }

    /// 对应 Java BatchOptions.executionMode(ExecutionMode)。
    pub fn execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

    /// 对应 Java BatchOptions.skipResult()。
    /// 跳过响应结果，适合 fire-and-forget 场景。
    pub fn skip_result(mut self) -> Self {
        self.skip_result = true;
        self
    }

    /// 对应 Java BatchOptions.responseTimeout(long timeout, TimeUnit unit)。
    pub fn response_timeout(mut self, timeout: Duration) -> Self {
        self.response_timeout = Some(timeout);
        self
    }

    /// 对应 Java BatchOptions.retryAttempts(int retryAttempts)。
    /// -1 表示使用全局配置。
    pub fn retry_attempts(mut self, attempts: i32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    // --------------------------------------------------------
    // Getter 方法
    // --------------------------------------------------------

    /// 对应 Java BatchOptions.getExecutionMode()
    pub fn get_execution_mode(&self) -> &ExecutionMode {
        &self.execution_mode
    }

    /// 对应 Java BatchOptions.getSyncSlaves()
    pub fn get_sync_slaves(&self) -> u32 {
        self.sync_slaves
    }

    /// 对应 Java BatchOptions.getSyncLocals()
    pub fn get_sync_locals(&self) -> u32 {
        self.sync_locals
    }

    /// 对应 Java BatchOptions.getSyncTimeout()
    pub fn get_sync_timeout(&self) -> Duration {
        self.sync_timeout
    }

    /// 对应 Java BatchOptions.isSyncAOF()
    pub fn is_sync_aof(&self) -> bool {
        self.sync_aof
    }

    /// 对应 Java BatchOptions.isSkipResult()
    pub fn is_skip_result(&self) -> bool {
        self.skip_result
    }

    /// 对应 Java BatchOptions.getResponseTimeout()（返回毫秒，Rust 侧返回 Option<Duration>）
    pub fn get_response_timeout(&self) -> Option<Duration> {
        self.response_timeout
    }

    /// 对应 Java BatchOptions.getRetryAttempts()
    pub fn get_retry_attempts(&self) -> i32 {
        self.retry_attempts
    }

    // --------------------------------------------------------
    // 辅助判断
    // --------------------------------------------------------

    /// 对应 Java CommandBatchService.isRedisBasedQueue()。
    /// 当执行模式为 REDIS_READ_ATOMIC 或 REDIS_WRITE_ATOMIC 时返回 true。
    pub fn is_redis_based_queue(&self) -> bool {
        matches!(
            self.execution_mode,
            ExecutionMode::RedisReadAtomic | ExecutionMode::RedisWriteAtomic
        )
    }
}
