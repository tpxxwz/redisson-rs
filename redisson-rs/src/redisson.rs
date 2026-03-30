use crate::api::redisson_client::RedissonClient;
use crate::command::command_async_service::CommandAsyncService;
use crate::config::RedissonConfig;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::ext::RedisKey;
use crate::redisson_batch::RedissonBatch;
use crate::redisson_bucket::RedissonBucket;
use crate::redisson_lock::RedissonLock;
use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
use anyhow::Result;
use std::sync::Arc;

// ============================================================
// Redisson — 对应 Java org.redisson.Redisson
// ============================================================

pub struct Redisson {
    connection_manager: Arc<dyn ConnectionManager>,
    command_executor: Arc<CommandAsyncService>,
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
    type RBucket = RedissonBucket<CommandAsyncService>;
    type RBatch = RedissonBatch;

    fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
        Arc::new(RedissonLock::new(&self.command_executor, name))
    }

    fn get_bucket<K: RedisKey>(&self, name: K) -> Self::RBucket {
        RedissonBucket::new(&self.command_executor, name)
    }

    fn create_batch(&self) -> Self::RBatch {
        RedissonBatch::new(self.command_executor.clone())
    }
}

impl RedissonClient for Arc<Redisson> {
    type RLock = RedissonLock<CommandAsyncService>;
    type RBucket = RedissonBucket<CommandAsyncService>;
    type RBatch = RedissonBatch;

    fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
        Arc::new(RedissonLock::new(&self.command_executor, name))
    }

    fn get_bucket<K: RedisKey>(&self, name: K) -> Self::RBucket {
        RedissonBucket::new(&self.command_executor, name)
    }

    fn create_batch(&self) -> Self::RBatch {
        RedissonBatch::new(self.command_executor.clone())
    }
}

// ============================================================
// init — 对应 Java Redisson.create(config)
// ============================================================

pub async fn init(config: RedissonConfig) -> Result<Arc<Redisson>> {
    tracing::info!(
        "Redis lock watchdog timeout: {}s",
        config.lock_watchdog_timeout
    );

    // 1. 创建 ConnectionManager（ServiceManager 此时无 scheduler）
    let connection_manager = FredConnectionManager::init(&config).await?;

    // 2. 创建 executor
    let command_executor = Arc::new(CommandAsyncService::new(connection_manager.clone()));

    // 3. 对应 Java: connectionManager.getServiceManager().register(new LockRenewalScheduler(executor))
    let renewal_scheduler = Arc::new(LockRenewalScheduler::new(
        command_executor.clone(),
        config.lock_watchdog_timeout * 1000,
    ));
    connection_manager
        .service_manager()
        .register(renewal_scheduler);
    tracing::info!("Lock renewal scheduler registered");

    Ok(Arc::new(Redisson {
        connection_manager: connection_manager as Arc<dyn ConnectionManager>,
        command_executor,
        config,
    }))
}
