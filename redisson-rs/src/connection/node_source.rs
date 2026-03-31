use crate::client::redis_client::RedisClient;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::misc::redis_uri::RedisURI;
use std::sync::Arc;

// ============================================================
// NodeSource — 对应 Java org.redisson.connection.NodeSource
// ============================================================

/// 对应 Java NodeSource.Redirect 枚举。
/// 标记重定向类型：Cluster MOVED（槽永久迁移）/ ASK（迁移中临时重定向）/ REDIRECT（通用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Redirect {
    /// 对应 Java Redirect.MOVED
    Moved,
    /// 对应 Java Redirect.ASK
    Ask,
    /// 对应 Java Redirect.REDIRECT
    Redirect,
}

/// 对应 Java org.redisson.connection.NodeSource。
///
/// 携带路由决策所需的全部信息，在 `RedisExecutor` 中根据优先级逐一判断：
/// 1. redirect → 使用重定向地址（MOVED/ASK）
/// 2. entry → 直接使用指定的 MasterSlaveEntry
/// 3. redis_client → 通过 client 查找对应 entry
/// 4. slot → 按槽号路由（read ? getReadEntry : getWriteEntry）
#[derive(Clone, Debug, Default)]
pub struct NodeSource {
    /// 对应 Java NodeSource.slot（哈希槽，Cluster 分片键）
    pub slot: Option<u16>,
    /// 对应 Java NodeSource.addr（MOVED/ASK 重定向目标地址）
    pub addr: Option<RedisURI>,
    /// 对应 Java NodeSource.redisClient（精确指定目标客户端，如特定从节点）
    pub redis_client: Option<Arc<RedisClient>>,
    /// 对应 Java NodeSource.redirect（重定向类型）
    pub redirect: Option<Redirect>,
    /// 对应 Java NodeSource.entry（直接指定主从连接池容器）
    pub entry: Option<Arc<MasterSlaveEntry>>,
}

impl NodeSource {
    /// 对应 Java new NodeSource(MasterSlaveEntry entry)
    pub fn from_entry(entry: Arc<MasterSlaveEntry>) -> Self {
        Self { entry: Some(entry), ..Default::default() }
    }

    /// 对应 Java new NodeSource(Integer slot)
    pub fn from_slot(slot: u16) -> Self {
        Self { slot: Some(slot), ..Default::default() }
    }

    /// 对应 Java new NodeSource(MasterSlaveEntry entry, RedisClient redisClient)
    pub fn from_entry_and_client(entry: Arc<MasterSlaveEntry>, redis_client: Arc<RedisClient>) -> Self {
        Self {
            entry: Some(entry),
            redis_client: Some(redis_client),
            ..Default::default()
        }
    }

    /// 对应 Java new NodeSource(RedisClient redisClient)
    pub fn from_client(redis_client: Arc<RedisClient>) -> Self {
        Self { redis_client: Some(redis_client), ..Default::default() }
    }

    /// 对应 Java new NodeSource(Integer slot, RedisClient redisClient)
    pub fn from_slot_and_client(slot: u16, redis_client: Arc<RedisClient>) -> Self {
        Self {
            slot: Some(slot),
            redis_client: Some(redis_client),
            ..Default::default()
        }
    }

    /// 对应 Java new NodeSource(Integer slot, RedisURI addr, Redirect redirect)
    pub fn from_redirect(slot: u16, addr: RedisURI, redirect: Redirect) -> Self {
        Self {
            slot: Some(slot),
            addr: Some(addr),
            redirect: Some(redirect),
            ..Default::default()
        }
    }

    /// 对应 Java new NodeSource(NodeSource nodeSource, RedisClient redisClient)
    pub fn from_node_source_with_client(source: &NodeSource, redis_client: Arc<RedisClient>) -> Self {
        Self {
            slot: source.slot,
            addr: source.addr.clone(),
            redis_client: Some(redis_client),
            redirect: source.redirect.clone(),
            entry: source.entry.clone(),
        }
    }
}
