use super::connection_manager::ConnectionManager;
use super::service_manager::ServiceManager;
use crate::config::server_mode::ServerMode;
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use crate::config::{
    RedissonConfig, build_connection_config, build_fred_config, build_perf_config,
};
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::{Context, Result};
use async_trait::async_trait;
use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, EventInterface, PubsubInterface};
use fred::prelude::{Pool, ReconnectPolicy};
use std::sync::Arc;

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

/// 所有字段均为 Arc<...> + Send + Sync，故 FredConnectionManager 也满足 Send + Sync。
/// 这使得 &CommandAsyncService 能在 async move 中安全捕获，满足 trait impl 返回 impl Future + Send + 'static 的约束。
unsafe impl Send for FredConnectionManager {}
unsafe impl Sync for FredConnectionManager {}

impl FredConnectionManager {
    /// 对应 Java MasterSlaveConnectionManager(MasterSlaveServersConfig, Config, UUID id)：
    /// 内部完成连接池、订阅客户端、PublishSubscribeService、ServiceManager 的初始化。
    pub async fn init(config: &RedissonConfig) -> Result<Arc<Self>> {
        let reconnect_policy = ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        );

        tracing::info!(
            "Connecting to Redis [mode={}] with pool_size={}",
            config.mode.as_str(),
            config.pool_size
        );

        let pool = Pool::new(
            build_fred_config(config)?,
            Some(build_perf_config(config)),
            Some(build_connection_config(config)),
            Some(reconnect_policy.clone()),
            config.pool_size,
        )
        .context("Failed to create Redis pool")?;

        pool.init().await.context("Failed to connect to Redis")?;
        tracing::info!("Redis connection pool established");

        let subscriber = SubscriberClient::new(
            build_fred_config(config)?,
            Some(build_perf_config(config)),
            Some(build_connection_config(config)),
            Some(reconnect_policy),
        );
        subscriber
            .init()
            .await
            .context("Failed to connect subscriber")?;

        let subscriber_clone = subscriber.clone();
        tokio::spawn(async move {
            let _ = subscriber_clone.manage_subscriptions().await;
        });
        tracing::info!("Redis Pub/Sub subscriber established");

        let publish_command = Self::check_sharding_support(&pool, config).await;
        let subscribe_service = Arc::new(PublishSubscribeService::new(
            subscriber.clone(),
            publish_command,
        ));
        tracing::info!("PublishSubscribeService initialized");

        let service_manager = Arc::new(ServiceManager::new(
            config.name_mapper.clone(),
            Arc::new(config.clone()),
            config.subscription_timeout,
            config.command_timeout_ms,
            config.retry_attempts,
            config.retry_delay.clone(),
        ));

        let lock_pub_sub = LockPubSub::new(subscribe_service.clone());
        tokio::spawn(Self::pubsub_message_listener(subscriber, lock_pub_sub));

        let use_replica_for_reads = matches!(config.mode, ServerMode::Cluster { .. })
            && config.read_from_slave;
        let config = Arc::new(config.clone());

        Ok(Arc::new(Self {
            pool,
            subscribe_service,
            service_manager,
            config,
            use_replica_for_reads,
        }))
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

    async fn pubsub_message_listener(subscriber: SubscriberClient, lock_pub_sub: LockPubSub) {
        let mut message_rx = subscriber.message_rx();
        tracing::info!("Pub/Sub message listener started");

        while let Ok(message) = message_rx.recv().await {
            let channel: &str = &message.channel;
            let msg_value: Option<i64> = message.value.convert().ok();
            lock_pub_sub.on_message(channel, msg_value);
        }
    }

}

#[async_trait]
impl ConnectionManager for FredConnectionManager {
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        &self.subscribe_service
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        &self.service_manager
    }

    async fn shutdown(&self) {
        self.service_manager.renewal_scheduler().shutdown();
        let _ = self.pool.quit().await;
    }

    /// 对应 Java ConnectionManager.calcSlot(String/ByteBuf/byte[] key)
    ///
    /// 注意：Java 中 ClusterConnectionManager 自行实现 CRC16 + hash tag 提取，
    /// MasterSlaveConnectionManager 直接返回 singleSlotRange.getStartSlot()（固定值 0），
    /// 两者行为通过多态区分。
    ///
    /// Rust 这边统一委托给 fred::util::redis_keyslot（redis-protocol crate 的标准实现，
    /// 逻辑与 ClusterConnectionManager 完全一致）。MasterSlave 的固定 slot 分支不需要，
    /// 原因：① fred Pool 内部屏蔽了 Cluster/MasterSlave 的节点路由差异，calc_slot 不再
    /// 参与路由决策；② 唯一调用方（rename 跨 slot 检查）已被 is_cluster_config() 前置
    /// 守卫，非 cluster 模式下此方法根本不会执行。
    fn calc_slot(&self, key: &[u8]) -> u16 {
        fred::util::redis_keyslot(key)
    }

    fn config(&self) -> &Arc<RedissonConfig> {
        &self.config
    }
}
