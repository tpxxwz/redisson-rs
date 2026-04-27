use fred::types::config::Server;
use std::hash::{Hash, Hasher};

// ============================================================
// MasterSlaveEntry — 对应 Java org.redisson.connection.MasterSlaveEntry
// ============================================================

/// 对应 Java MasterSlaveEntry：代表一个主节点及其从节点的逻辑分组。
///
/// Java 侧持有物理连接池和从节点列表，并在节点 up/down 时动态更新；
/// Rust 侧连接池由 fred 内部管理，这里只保留识别和路由所需的最少信息：
///
/// - `primary`：构造时从 fred 当前路由表快照解析，仅用于 Hash/Eq 分组，
///   **不用于实际命令路由**（路由由 fred 通过 ClusterHash::Custom(slot) 完成）。
/// - `slot`：cluster 模式下对应的 slot，fred 实际路由时使用。
///
/// 因此即使发生 failover，实际命令仍由 fred 路由到最新的 primary，
/// `primary` 字段只影响 batch 分组的准确性（同一 primary 的命令合并到一个 pipeline）。
#[derive(Clone, Debug)]
pub struct MasterSlaveEntry {
    /// 主节点地址快照，仅用于 Hash / Eq / Display，不直接用于建立连接。
    pub(crate) primary: Server,
    /// 对应的 cluster slot（单机/哨兵模式下为 None）。
    pub(crate) slot: Option<u16>,
}

impl MasterSlaveEntry {
    /// 对应 Java new MasterSlaveEntry(ConnectionManager, MasterSlaveServersConfig)。
    /// Cluster 模式下由 slot + 当时的路由表快照构造。
    pub fn from_slot(slot: u16, primary: Server) -> Self {
        Self { primary, slot: Some(slot) }
    }

    /// 单机 / 哨兵 / 主从模式下构造，无 slot 概念。
    pub fn from_server(primary: Server) -> Self {
        Self { primary, slot: None }
    }

    /// 对应 Java MasterSlaveEntry.getClient()：返回主节点标识。
    /// 注意：返回的是构造时的快照，实际路由仍由 fred 动态完成。
    pub fn get_client(&self) -> &Server {
        &self.primary
    }

    /// 对应的 cluster slot（单机模式下为 None）。
    pub fn slot(&self) -> Option<u16> {
        self.slot
    }
}

/// Hash / Eq 基于 primary（host + port），保证同一主节点的 Entry 在 HashMap 中合并。
impl Hash for MasterSlaveEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.primary.host.hash(state);
        self.primary.port.hash(state);
    }
}

impl PartialEq for MasterSlaveEntry {
    fn eq(&self, other: &Self) -> bool {
        self.primary.host == other.primary.host && self.primary.port == other.primary.port
    }
}

impl Eq for MasterSlaveEntry {}
