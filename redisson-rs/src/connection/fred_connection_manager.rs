use super::connection_manager::ConnectionManager;
use super::service_manager::ServiceManager;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::config::{
    RedissonConfig, ServerMode, build_connection_config, build_fred_config, build_perf_config,
};
// use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::{Context, Result};
use async_trait::async_trait;
// use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, EventInterface, PubsubInterface};
use fred::prelude::{Pool, ReconnectPolicy};
use std::sync::{Arc, Weak};
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
// ============================================================
// FredConnectionManager
// ============================================================

/// 基于 fred 客户端的连接管理器。
/// Java 中针对不同模式有 MasterSlaveConnectionManager / ClusterConnectionManager /
/// SentinelConnectionManager 等多个实现类；Rust 这里 fred Pool 统一支持
/// standalone / cluster / sentinel，无需拆分，故统一命名为 FredConnectionManager。
pub struct FredConnectionManager {
    /// Redis 命令连接池（fred Pool），支持 standalone / cluster / sentinel
    pub(crate) pool: Pool,
    /// Pub/Sub 订阅服务，对应 Java subscribeService
    pub(crate) subscribe_service: Arc<PublishSubscribeService>,
    /// 服务管理器，对应 Java serviceManager
    pub(crate) service_manager: Arc<ServiceManager>,
    /// 对应 Java ServiceManager.cfg (Config)
    pub(crate) config: Arc<RedissonConfig>,
    /// 是否从 replica 读取（仅 cluster + read_from_slave=true 时为 true）
    pub(crate) use_replica_for_reads: bool,
}

impl FredConnectionManager {
    /// 对应 Java MasterSlaveConnectionManagxer(MasterSlaveServersConfig, Config, UUID id)：
    /// 内部完成连接池、订阅客户端、PublishSubscribeService、ServiceManager 的初始化。
    pub async fn create(config: RedissonConfig) -> Result<Arc<Self>> {
        let reconnect_policy = ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        );

        tracing::info!(
            "Connecting to Redis [mode={:?}] with pool_size={}",
            config.mode,
            config.pool_size
        );

        let pool = Pool::new(
            build_fred_config(&config)?,
            Some(build_perf_config(&config)),
            Some(build_connection_config(&config)),
            Some(reconnect_policy.clone()),
            config.pool_size,
        )
        .context("Failed to create Redis pool")?;

        pool.init().await.context("Failed to connect to Redis")?;
        tracing::info!("Redis connection pool established");

        let publish_command = Self::check_sharding_support(&pool, &config).await;

        let use_replica_for_reads = matches!(config.mode, ServerMode::Cluster { .. })
            && config.read_from_slave;
        let config = Arc::new(config);

        let service_manager = Arc::new(ServiceManager{});

        // 先创建 subscribe_service（内部完成建连），此时 connection_manager 尚未构建
        let subscribe_service = PublishSubscribeService::new(config.clone(), publish_command).await?;
        tracing::info!("PublishSubscribeService initialized");

        let connection_manager = Arc::new(Self {
            pool,
            subscribe_service: subscribe_service.clone(),
            service_manager,
            config,
            use_replica_for_reads,
        });

        subscribe_service.set_connection_manager(
            Arc::downgrade(&connection_manager) as Weak<dyn ConnectionManager>,
        );

        Ok(connection_manager)
    }

    pub fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    pub fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }

    /// 对应 Java ClusterConnectionManager.checkShardingSupport()
    async fn check_sharding_support(pool: &Pool, config: &RedissonConfig) -> &'static str {
        if !matches!(config.mode, ServerMode::Cluster { .. }) {
            return "publish";
        }
        match config.sharded_subscription_mode {
            ShardedSubscriptionMode::Off => "publish",
            ShardedSubscriptionMode::On => "spublish",
            ShardedSubscriptionMode::Auto => {
                let result: Result<fred::types::Value, _> = pool
                    .next()
                    .pubsub_shardnumsub::<fred::types::Value, _>(vec![""])
                    .await;
                if result.is_ok() {
                    tracing::info!("Sharded Pub/Sub supported, using SPUBLISH");
                    "spublish"
                } else {
                    tracing::info!("Sharded Pub/Sub not supported, using PUBLISH");
                    "publish"
                }
            }
        }
    }

}

#[async_trait]
impl ConnectionManager for FredConnectionManager {
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    async fn shutdown(&self) {
        // self.service_manager.renewal_scheduler().shutdown();
        let _ = self.pool.quit().await;
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }

    /// 对应 Java ConnectionManager.createCommandExecutor()
    ///
    /// self: Arc<Self> 对应 Java 的 this——Java 所有对象引用本质上都是 Arc（由 GC 管理），
    /// 传给 CommandAsyncService 的构造器等价于 Rust 把 Arc<Self> 直接交出去。
    fn create_command_executor(self: Arc<Self>) -> Arc<dyn CommandAsyncExecutor> {
        // crate::command::command_async_executor::create(
        //     self,
        //     RedissonObjectBuilder::default(),
        //     crate::liveobject::core::redisson_object_builder::ReferenceType::Default,
        // )
        unimplemented!()
    }

    fn use_replica_for_reads(&self) -> bool {
        self.use_replica_for_reads
    }

    fn config(&self) -> &Arc<RedissonConfig> {
        &self.config
    }
}
