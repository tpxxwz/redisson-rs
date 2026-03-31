use fred::interfaces::ClientLike;
use fred::prelude::Pool;
use fred::types::config::Server;

// ============================================================
// MasterSlaveEntry — 对应 Java org.redisson.connection.MasterSlaveEntry（类）
// ============================================================

/// 对应 Java org.redisson.connection.MasterSlaveEntry。
/// 代表一组主从节点（一个 master + 若干 slaves），是连接管理的基本单元。
///
/// 【Java vs Rust 的本质差异】
///
/// Java 中 MasterSlaveEntry 是一个重型连接池管理器：
///   - 内部维护 MasterConnectionPool / SlaveConnectionPool / PubSubConnectionPool
///   - 每条 Redis 命令都要走"借连接 → 发命令 → 还连接"的生命周期
///   - 命令链路：getWriteEntry(slot) → entry.connectionWriteOp(command)
///                → connection.async(command, args) → entry.releaseWrite(connection)
///
/// Rust 侧这些全部由 fred Pool 接管，MasterSlaveEntry 退化为轻量节点描述符：
///   - 连接借还       → fred Pool 内部管理，调用方直接 pool.custom(cmd, args).await
///   - PubSub 连接   → PublishSubscribeService 直接持有 SubscriberClient，不经过 entry
///   - 节点故障/切换  → fred ReconnectPolicy + Sentinel 支持，自动处理
///   - MOVED/ASK     → fred Cluster 模式自动重定向，不需要 Rust 侧介入
///
/// 【为什么还需要 MasterSlaveEntry】
///
/// 有一类操作需要"向每个节点各发一条命令"（全节点 SCAN、executeAll、RedissonKeys 等）：
///   Java:  getEntrySet() → for entry → entry.connectionWriteOp → connection.async
///   Rust:  get_entry_set() → for entry → entry.client_for_node().custom(cmd, args).await
///
/// entry.client_for_node() 返回一个路由到 self.server 的 fred 客户端，
/// 确保命令精确发到这个 entry 代表的节点，而非走 Pool 默认的轮询路由。
///
/// 【外部调用者分析与 fred 替代可行性】
///
/// - JCache：entry 只作为 commandExecutor.writeAsync(entry, ...) 的路由参数
///   → ✅ client_for_node().custom(...) 完全替代
///
/// - RedissonBaseNodes：取 getClient() / getAllEntries()，再通过 CommandAsyncExecutor 发命令
///   → ✅ entry.server 提供节点标识，命令通过 client_for_node() 发送
///
/// - RedissonTransaction：只调 entry.getAvailableSlaves() 查副本数量元数据
///   → ✅ 从 fred cluster state 查副本数替代
///
/// - RedissonNode：调 connectionReadOp() 拿连接，再取 conn.getChannel().remoteAddress()
///   → ✅ 实际要的是 TCP 地址，直接从 entry.server.host / entry.server.port 取即可，
///        无需真的建连再从 Channel 读地址
///
/// - PublishSubscribeService：调 nextPubSubConnection() 拿 RedisPubSubConnection，
///   再对连接调 conn.addListener() / conn.subscribe()
///   → ✅ Rust 侧 PublishSubscribeService 已直接持有 SubscriberClient，完全绕开 entry，
///        此路径在 Rust 侧不存在
pub struct MasterSlaveEntry {
    /// 本 entry 代表的 master 节点（host + port）
    pub server: Server,
    /// 共享的 fred Pool（Pool 内部已是 RefCount<PoolInner>，clone 为 O(1) 引用计数操作，
    /// 无需再套一层 Arc）
    ///
    /// 非 Cluster 模式：Pool 只有一个节点，client_for_node() 退化为 pool.next()
    /// Cluster 模式：client_for_node() 通过 with_cluster_node(&self.server) 精确路由
    pool: Pool,
}

impl MasterSlaveEntry {
    pub fn new(server: Server, pool: Pool) -> Self {
        Self { server, pool }
    }

    /// 返回一个路由到本节点的 fred 客户端。
    ///
    /// 调用方通过此客户端发命令，命令会精确发到 self.server 节点：
    ///   entry.client_for_node().custom(cmd, args).await
    ///
    /// Java 中命令执行涉及 connectionWriteOp / sendCommand / releaseConnection 三个步骤，
    /// 但它们并非连续调用——中间分散着重试超时、写超时、MOVED/ASK 重定向、节点故障检测、
    /// 阻塞命令处理等大量独立逻辑，connection 对象在整个异步生命周期里被多处持有和引用。
    /// Rust 侧这套复杂机制由 fred 在内部统一处理，调用方只需 .custom(cmd, args).await，
    /// 连接借还、重试、错误处理均由 fred 负责，无需外部介入。
    pub fn client_for_node(&self) -> impl ClientLike + use<'_> {
        self.pool.next().with_cluster_node(&self.server)
    }

    /// 返回 master 节点标识，对应 Java MasterSlaveEntry.getClient().getAddr()。
    ///
    /// Java 中 getClient() 返回一个 RedisClient 对象，通常只是为了取地址；
    /// Rust 侧直接返回 &Server（包含 host + port），避免引入无意义的包装层。
    pub fn server(&self) -> &Server {
        &self.server
    }
}
