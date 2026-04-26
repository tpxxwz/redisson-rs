use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use fred::types::config::Options;
use fred::types::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

// 对应 Java CommandAsyncService.SORT_RO_SUPPORTED。
// 只读模式下执行 SORT 时，优先尝试 SORT_RO（Redis 7.0+）；
// 遇到 ERR unknown command 则置 false，后续直接走普通 SORT。
pub(crate) static SORT_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);

// 对应 Java CommandAsyncService.EVAL_SHA_RO_SUPPORTED。
// 只读模式下执行 Lua 脚本时，优先尝试 EVALSHA_RO（Redis 7.0+）；
// 遇到 ERR unknown command 则置 false，后续降级到 EVALSHA 或完整 EVAL。
pub(crate) static EVAL_SHA_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);

pub(crate) trait CommandAsyncServiceLike: Send + Sync {
    fn inner(&self) -> &CommandAsyncInner;

    /// 对应 Java CommandAsyncService.async()
    async fn async_execute(&self, cmd: RedisCommand) -> anyhow::Result<Value> {
        let pool = &self.inner().connection_manager.pool;
        let options = self.inner().build_options();
        cmd.execute(pool, &options).await
    }

    fn is_eval_cache_active(&self) -> bool {
        self.inner().connection_manager.config().use_script_cache
    }

    fn is_batch(&self) -> bool {
        false
    }
}

pub struct CommandAsyncInner {
    pub connection_manager: Arc<FredConnectionManager>,
    pub retry_attempts: Option<u32>,
    pub response_timeout: Option<Duration>,
    pub track_changes: bool,
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
        }
    }

    /// 将 retry_attempts / response_timeout 组装成 fred Options，
    /// 供 RedisCommand::execute() 使用。
    pub(crate) fn build_options(&self) -> Options {
        Options {
            max_attempts: self.retry_attempts,
            timeout:      self.response_timeout,
            ..Default::default()
        }
    }

    async fn synced_eval(&self) -> anyhow::Result<Value> {
        unimplemented!()
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
