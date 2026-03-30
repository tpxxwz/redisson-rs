// ============================================================
// ServerMode — 对应 Java 中通过不同 ConnectionManager 类型区分的连接模式
// (SingleConnectionManager / ClusterConnectionManager / SentinelConnectionManager)
// ============================================================

use crate::config::RedisNode;

/// Redis 服务器连接模式，每个变体携带自身所需的连接信息。
#[derive(Clone)]
pub enum ServerMode {
    Standalone {
        server: RedisNode,
        db: u8,
    },
    Cluster {
        nodes: Vec<RedisNode>,
    },
    Sentinel {
        sentinels: Vec<RedisNode>,
        service_name: String,
        username: Option<String>,
        password: Option<String>,
        db: u8,
    },
}

impl ServerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerMode::Standalone { .. } => "standalone",
            ServerMode::Cluster { .. } => "cluster",
            ServerMode::Sentinel { .. } => "sentinel",
        }
    }
}
