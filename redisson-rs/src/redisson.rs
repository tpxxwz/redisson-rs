use crate::api::redisson_client::RedissonClient;
use crate::command::command_async_service::CommandAsyncService;
use crate::config::RedissonConfig;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::ext::RedisKey;
use crate::redisson_lock::RedissonLock;
use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
use anyhow::Result;
use std::sync::Arc;
use crate::command::command_async_executor::CommandAsyncExecutor;
// ============================================================
// Redisson — 对应 Java org.redisson.Redisson
// ============================================================

pub struct Redisson {
    connection_manager: Arc<dyn ConnectionManager>,
    command_executor: Arc<dyn CommandAsyncExecutor>,
    config: RedissonConfig,
}

impl Redisson {
    pub fn connection_manager(&self) -> &Arc<dyn ConnectionManager> {
        &self.connection_manager
    }

    pub fn command_executor(&self) -> &Arc<CommandAsyncService> {
        &self.command_executor
    }

    pub fn config(&self) -> &RedissonConfig {
        &self.config
    }
}

impl RedissonClient for Redisson {
    type RLock = RedissonLock<CommandAsyncService>;

    fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
        Arc::new(RedissonLock::new(&self.command_executor, name))
    }
}

impl RedissonClient for Arc<Redisson> {
    type RLock = RedissonLock<CommandAsyncService>;

    fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
        Arc::new(RedissonLock::new(&self.command_executor, name))
    }

}

// ============================================================
// init — 对应 Java Redisson.create(config)
// ============================================================

pub async fn init(config: RedissonConfig) -> Result<Arc<Redisson>> {
    // 1. 创建 ConnectionManager（ServiceManager 此时无 scheduler）
    let connection_manager = FredConnectionManager::init(&config).await?;

    // 2. 创建 executor（对应 Java: connectionManager.createCommandExecutor(objectBuilder, ReferenceType.DEFAULT)）
    // 直接构造 CommandAsyncService，保留具体类型供 LockRenewalScheduler 等内部结构使用
    let command_executor = Arc::new(CommandAsyncService::new(connection_manager.clone()));

    // 3. 对应 Java: connectionManager.getServiceManager().register(new LockRenewalScheduler(executor))
    let lock_renewal_scheduler = Arc::new(LockRenewalScheduler::new(
        command_executor.clone()
    ));
    connection_manager
        .service_manager()
        .register(lock_renewal_scheduler);
    Ok(Arc::new(Redisson {
        connection_manager: connection_manager as Arc<dyn ConnectionManager>,
        command_executor,
        config,
    }))
}
