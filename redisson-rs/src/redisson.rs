use crate::api::redisson_client::RedissonClient;
use crate::api::rpattern_topic::RedissonPatternTopic;
use crate::command::command_async_service::CommandAsyncService;
use crate::config::RedissonConfig;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
// use crate::ext::RedisKey;
// use crate::redisson_lock::RedissonLock;
// use crate::renewal::lock_renewal_scheduler::LockRenewalScheduler;
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
    /// 对应 Java Redisson.create(Config config)
    pub async fn create(config: RedissonConfig) -> Result<Arc<Self>> {
        let connection_manager = FredConnectionManager::create(config.clone()).await?;

        let command_executor = Arc::new(CommandAsyncService::new(connection_manager.clone()));

        // let lock_renewal_scheduler = Arc::new(LockRenewalScheduler::new(
        //     command_executor.clone()
        // ));
        // connection_manager
        //     .service_manager()
        //     .register(lock_renewal_scheduler);

        Ok(Arc::new(Self {
            connection_manager: connection_manager as Arc<dyn ConnectionManager>,
            command_executor: command_executor as Arc<dyn CommandAsyncExecutor>,
            config,
        }))
    }

    pub fn connection_manager(&self) -> &Arc<dyn ConnectionManager> {
        &self.connection_manager
    }

    pub fn command_executor(&self) -> &Arc<dyn CommandAsyncExecutor> {
        &self.command_executor
    }

    pub fn config(&self) -> &RedissonConfig {
        &self.config
    }

    /// 对应 Java Redisson.getPatternTopic(String pattern)
    pub fn get_pattern_topic(&self, pattern: impl Into<String>) -> RedissonPatternTopic {
        RedissonPatternTopic::new(self.command_executor.clone(), pattern.into())
    }
}

// impl RedissonClient for Redisson {
//     type RLock = RedissonLock<CommandAsyncService>;
// 
//     fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
//         Arc::new(RedissonLock::new(&self.command_executor, name))
//     }
// }
// 
// impl RedissonClient for Arc<Redisson> {
//     type RLock = RedissonLock<CommandAsyncService>;
// 
//     fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock> {
//         Arc::new(RedissonLock::new(&self.command_executor, name))
//     }
// 
// }

