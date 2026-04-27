use crate::connection::master_slave_entry::MasterSlaveEntry;
use fred::types::config::Server;

// ============================================================
// NodeSource — 对应 Java org.redisson.connection.NodeSource
// ============================================================

/// 对应 Java NodeSource.Redirect
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Redirect {
    /// 对应 Java Redirect.MOVED
    Moved,
    /// 对应 Java Redirect.ASK
    Ask,
}

/// 对应 Java NodeSource：命令路由来源，描述"这条命令应该发往哪个节点"。
///
/// Java 侧支持按 slot / MasterSlaveEntry / RedisClient 三种方式寻址；
/// Rust 侧 RedisClient 概念由 fred 内部管理，外部不感知，
/// 故用 `Server`（host + port）替代 RedisClient 作为具体节点标识。
#[derive(Clone, Debug)]
pub struct NodeSource {
    /// 按 MasterSlaveEntry 寻址（已解析到具体主节点）
    entry: Option<MasterSlaveEntry>,
    /// 按 slot 寻址（cluster 模式，路由表尚未解析或不需要解析时使用）
    slot: Option<u16>,
    /// 具体节点地址（对应 Java 的 RedisClient，用于 MOVED/ASK redirect 场景）
    server: Option<Server>,
    /// MOVED / ASK redirect 标记
    redirect: Option<Redirect>,
}

impl NodeSource {
    /// 对应 Java new NodeSource(Integer slot)
    pub fn from_slot(slot: u16) -> Self {
        Self { entry: None, slot: Some(slot), server: None, redirect: None }
    }

    /// 对应 Java new NodeSource(MasterSlaveEntry entry)
    pub fn from_entry(entry: MasterSlaveEntry) -> Self {
        Self { entry: Some(entry), slot: None, server: None, redirect: None }
    }

    /// 对应 Java new NodeSource(MasterSlaveEntry entry, RedisClient redisClient)
    pub fn from_entry_and_server(entry: MasterSlaveEntry, server: Server) -> Self {
        Self { entry: Some(entry), slot: None, server: Some(server), redirect: None }
    }

    /// 对应 Java new NodeSource(Integer slot, RedisURI addr, Redirect redirect)
    pub fn from_slot_redirect(slot: u16, server: Server, redirect: Redirect) -> Self {
        Self { entry: None, slot: Some(slot), server: Some(server), redirect: Some(redirect) }
    }

    /// 对应 Java NodeSource.getEntry()
    pub fn get_entry(&self) -> Option<&MasterSlaveEntry> {
        self.entry.as_ref()
    }

    /// 对应 Java NodeSource.getSlot()
    pub fn get_slot(&self) -> Option<u16> {
        self.slot
    }

    /// 对应 Java NodeSource.getRedisClient()（Rust 侧用 Server 替代）
    pub fn get_server(&self) -> Option<&Server> {
        self.server.as_ref()
    }

    /// 对应 Java NodeSource.getRedirect()
    pub fn get_redirect(&self) -> Option<&Redirect> {
        self.redirect.as_ref()
    }
}
