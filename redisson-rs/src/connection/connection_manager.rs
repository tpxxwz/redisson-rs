use crate::api::node_type::NodeType;
use crate::client::redis_client::RedisClient;
use crate::config::RedissonConfig;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
use crate::liveobject::core::redisson_object_builder::{RedissonObjectBuilder, ReferenceType};
use crate::misc::redis_uri::RedisURI;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// ============================================================
// ConnectionManager — 对应 Java org.redisson.connection.ConnectionManager（接口）
// ============================================================

/// 连接管理器接口，对应 Java org.redisson.connection.ConnectionManager。
///
/// 【为什么用 Arc<dyn ConnectionManager> 而不是泛型】
///
/// - 运行时根据配置选一种实现，整个 runtime 全局唯一：
///     Single 模式   → FredConnectionManager（当前实现）
///     Cluster 模式  → ClusterConnectionManager（待实现）
///     Sentinel 模式 → SentinelConnectionManager（待实现）
///   启动时一次性确定，之后不变，Arc<dyn> 足够且最自然。
///
/// - 若改用泛型 CM，ConnectionManager 会向上传染：
///   CommandAsyncService<CM> → Redisson<CM> → 所有持有方都要带 CM 泛型，
///   而 CM 永远只在启动时确定一次，传染带来的复杂度完全没有收益。
///
/// - #[async_trait] 将 async fn 改写成 Pin<Box<dyn Future>>，
///   使 trait 对象安全，每次调用有一次 Box 堆分配，
///   但 ConnectionManager 调用频率低（连接管理层），开销可忽略不计。
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// 对应 Java ConnectionManager.connect()
    async fn connect(&self) {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getSubscribeService()
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService>;

    /// 对应 Java ConnectionManager.getLastClusterNode()
    fn get_last_cluster_node(&self) -> Option<RedisURI> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.calcSlot(String/ByteBuf/byte[] key)
    /// 计算 key 的 cluster hash slot（0–16383）。
    /// &[u8] 统一覆盖 Java 三个重载，调用方 .as_bytes() 即可。
    fn calc_slot(&self, key: &[u8]) -> u16;

    /// 对应 Java ConnectionManager.getEntrySet()
    fn get_entry_set(&self) -> Vec<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getEntry(String name)
    fn get_entry_by_name(&self, _name: &str) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getEntry(int slot)
    fn get_entry_by_slot(&self, _slot: u16) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getWriteEntry(int slot)
    fn get_write_entry(&self, _slot: u16) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getReadEntry(int slot)
    fn get_read_entry(&self, _slot: u16) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getEntry(InetSocketAddress address)
    fn get_entry_by_address(&self, _address: SocketAddr) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getEntry(RedisURI addr)
    fn get_entry_by_uri(&self, _addr: &RedisURI) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getEntry(RedisClient redisClient)
    fn get_entry_by_client(&self, _client: &RedisClient) -> Option<Arc<MasterSlaveEntry>> {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.createClient(NodeType, InetSocketAddress, RedisURI, String)
    fn create_client_with_address(
        &self,
        _node_type: NodeType,
        _address: SocketAddr,
        _uri: &RedisURI,
        _ssl_hostname: Option<&str>,
    ) -> RedisClient {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.createClient(NodeType, RedisURI, String)
    fn create_client(
        &self,
        _node_type: NodeType,
        _address: &RedisURI,
        _ssl_hostname: Option<&str>,
    ) -> RedisClient {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.shutdown()
    async fn shutdown(&self);

    /// 对应 Java ConnectionManager.shutdown(long quietPeriod, long timeout, TimeUnit unit)
    async fn shutdown_with_timeout(&self, _quiet_period: Duration, _timeout: Duration) {
        unimplemented!()
    }

    /// 对应 Java ConnectionManager.getServiceManager()
    fn service_manager(&self) -> &Arc<ServiceManager>;

    /// 对应 Java ConnectionManager.createCommandExecutor(RedissonObjectBuilder, ReferenceType)
    /// 注意：CommandAsyncExecutor 在 Rust 中为非对象安全 trait（含 impl Future 返回），
    /// 此处以 Box<dyn Any + Send + Sync> 占位，实现方返回具体执行器类型。
    fn create_command_executor(
        &self,
        _object_builder: &RedissonObjectBuilder,
        _reference_type: ReferenceType,
    ) -> Box<dyn std::any::Any + Send + Sync> {
        unimplemented!()
    }

    /// 对应 Java connectionManager.getServiceManager().getCfg()
    fn config(&self) -> &Arc<RedissonConfig>;
}
