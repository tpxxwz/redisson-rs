use crate::api::node_type::NodeType;
use crate::client::redis_client::RedisClient;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::config::RedissonConfig;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
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
///
/// 【为什么部分方法是 async fn】
///
/// - Rust 运行时采用 Tokio 全异步模型，不存在"阻塞等待"选项，
///   阻塞 Tokio worker 线程会卡住整个 runtime。
/// - Java Redisson 的 connect() / shutdown() 虽然对外暴露同步阻塞接口，
///   但底层依赖 Netty EventLoop 做异步 I/O，最终在调用线程 .join() 收口；
///   Rust 这边没有这层包装，必须直接 .await。
/// - 具体驱动：底层 fred 库的对应方法（Pool::init、Pool::quit 等）本身就是
///   async fn，实现层需要 .await，因此 trait 定义层也必须声明为 async fn。
/// - #[async_trait] 只对标注了 async fn 的方法生效，其余同步方法签名不受影响。
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
    ///
    /// 【为什么 receiver 是 self: Arc<Self> 而不是 &self】
    ///
    /// CommandAsyncService 持有 Arc<dyn ConnectionManager>（对应 Java 的 this 引用）。
    /// Java 的 this 本质上等价于 Rust 的 Arc<Self>——对象已在堆上，可随时共享引用。
    /// Rust 中 &self 只是借用，无法凭借它 clone 出 Arc 传给 CommandAsyncService；
    /// 改用 Arc<Self> 作为 receiver，self 本身就是 Arc，直接传入即可，与 Java 语义完全对齐。
    ///
    /// Arc<Self> 是 Rust 允许的 dyn-safe receiver 类型，可通过 Arc<dyn ConnectionManager> 调用。
    fn create_command_executor(self: Arc<Self>) -> Arc<dyn CommandAsyncExecutor>;

    /// 是否从 replica 读取（仅 Cluster + read_from_slave=true 时为 true）
    fn use_replica_for_reads(&self) -> bool {
        false
    }

    /// 对应 Java connectionManager.getServiceManager().getCfg()
    fn config(&self) -> &Arc<RedissonConfig>;
}
