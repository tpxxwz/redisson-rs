use super::connection_manager::ConnectionManager;
use super::service_manager::ServiceManager;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::config::{
    RedissonConfig, ServerMode, build_connection_config, build_fred_config, build_perf_config,
};
use crate::config::read_mode::ReadMode;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use anyhow::{Context, Result};
use async_trait::async_trait;
use fred::interfaces::{ClientLike, ClusterInterface, EventInterface, PubsubInterface};
use fred::prelude::{Pool, ReconnectPolicy};
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use std::sync::{Arc, OnceLock};
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
    pub(crate) subscribe_service: OnceLock<Arc<PublishSubscribeService>>,
    /// 服务管理器，对应 Java serviceManager
    pub(crate) service_manager: Arc<ServiceManager>,
    /// 对应 Java ServiceManager.cfg (Config)
    pub(crate) config: Arc<RedissonConfig>,
    /// 对应 Java BaseMasterSlaveServersConfig.readMode
    pub(crate) read_mode: ReadMode,
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

        // Pool 里所有 client 共享同一事件总线，注册在第一个 client 上即可。
        let first_client = pool.next().clone();

        // 集群拓扑变更时清空脚本缓存，对应 Java ServiceManager 里监听 cluster 事件后清 SCRIPT_SHA_CACHE。
        // Add/Remove/Rebalance 都意味着 slot→node 映射可能变化，旧的 per-node 脚本缓存全部失效。
        first_client.on_cluster_change(|_changes| async move {
            ServiceManager::clear_all_script_caches();
            Ok(())
        });

        // 单节点重连时清除该节点的脚本缓存（已在 register_reconnect_listener 里处理），
        // 这里统一注册一次即可。
        ServiceManager::register_reconnect_listener(&first_client);

        let publish_command = Self::check_sharding_support(&pool, &config).await;

        let read_mode = config.read_mode.clone();
        let config = Arc::new(config);

        let service_manager = Arc::new(ServiceManager{});

        let connection_manager = Arc::new(Self {
            pool,
            subscribe_service: OnceLock::new(),
            service_manager,
            config,
            read_mode,
        });

        let connection_manager_weak = Arc::downgrade(&(connection_manager.clone() as Arc<dyn ConnectionManager>));
        let subscribe_service = PublishSubscribeService::new(
            connection_manager_weak,
            connection_manager.config.clone(),
            publish_command,
        )
        .await?;
        tracing::info!("PublishSubscribeService initialized");

        connection_manager
            .subscribe_service
            .set(subscribe_service)
            .map_err(|_| anyhow::anyhow!("subscribe_service already set"))?;

        Ok(connection_manager)
    }

    pub fn subscribe_service(&self) -> &Arc<PublishSubscribeService> {
        self.subscribe_service
            .get()
            .expect("subscribe_service not initialized")
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
        self.subscribe_service()
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

    fn config(&self) -> &Arc<RedissonConfig> {
        &self.config
    }

    /// 对应 Java ClusterConnectionManager.getWriteEntry(int slot) /
    ///         MasterSlaveConnectionManager.getWriteEntry(int slot)。
    fn get_write_entry(&self, slot: u16) -> Option<MasterSlaveEntry> {
        if let Some(routing) = self.pool.cached_cluster_state() {
            return routing.get_server(slot).cloned().map(|s| MasterSlaveEntry::from_slot(slot, s));
        }
        self.pool.next().client_config().server.hosts().into_iter().next()
            .map(MasterSlaveEntry::from_server)
    }

    /// 对应 Java MasterSlaveConnectionManager.getReadEntry(int slot)。
    /// ReadMode::Master 时等同于 get_write_entry；Slave / MasterSlave 时优先返回副本节点。
    fn get_read_entry(&self, slot: u16) -> Option<MasterSlaveEntry> {
        if matches!(self.read_mode, ReadMode::Master) {
            return self.get_write_entry(slot);
        }
        // Slave / MasterSlave：cluster 模式下从路由表取副本列表
        if let Some(routing) = self.pool.cached_cluster_state() {
            if let Some(primary) = routing.get_server(slot).cloned() {
                let replicas = routing.replicas(&primary);
                if !replicas.is_empty() {
                    let idx = slot as usize % replicas.len();
                    return Some(MasterSlaveEntry::from_slot(slot, replicas[idx].clone()));
                }
                return Some(MasterSlaveEntry::from_slot(slot, primary));
            }
        }
        // 非 cluster 模式：副本选择由执行层通过 pool.replicas() 完成
        self.get_write_entry(slot)
    }
}
