pub mod read_mode;
pub mod server_mode;
pub mod sharded_subscription_mode;
pub(crate) use crate::config::server_mode::ServerMode;
use crate::config::read_mode::ReadMode;
use crate::config::sharded_subscription_mode::ShardedSubscriptionMode;
use anyhow::Result;
use fred::types::config::{
    ClusterDiscoveryPolicy, Config, ConnectionConfig, PerformanceConfig, Server, ServerConfig,
};
use serde::Deserialize;
use std::time::Duration;

// ============================================================
// RedisNode
// ============================================================

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RedisNode {
    pub host: String,
    pub port: u16,
}

// ============================================================
// RedisConfig — 用于配置文件（YAML 等）反序列化，所有字段为基础类型
// ============================================================

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct RedisConfig {
    // ── fred::Config ──
    /// "standalone" | "cluster" | "sentinel"
    pub mode: String,
    pub host: String,
    pub port: u16,
    pub nodes: Vec<RedisNode>,
    pub sentinel_service_name: String,
    pub sentinel_username: Option<String>,
    pub sentinel_password: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub db: u8,

    // ── fred::PerformanceConfig ──
    pub command_timeout_secs: u64,

    // ── fred::ConnectionConfig ──
    pub connect_timeout_secs: u64,
    pub max_command_attempts: u32,
    pub max_redirections: u32,

    // ── fred::ReconnectPolicy ──
    pub reconnect_max_attempts: u32,
    pub reconnect_min_delay_ms: u32,
    pub reconnect_max_delay_ms: u32,
    pub reconnect_multiplier: u32,

    // ── fred::Pool / client count ──
    pub pool_size: usize,

    // ── Redisson 行为配置 ──
    pub lock_watchdog_timeout: u64,
    pub subscription_timeout: u64,
    pub command_timeout_ms: u64,
    pub retry_attempts: u32,
    pub retry_delay_base_ms: u64,
    pub retry_delay_max_ms: u64,

    // ── Pub/Sub ──
    /// "auto" | "on" | "off"
    pub sharded_subscription_mode: String,

    // ── 读策略 ──
    /// 对应 Java BaseMasterSlaveServersConfig.readMode，"slave"|"master"|"master_slave"
    pub read_mode: String,

    // ── Script 缓存 ──
    /// 是否启用 EVALSHA 脚本缓存（对应 Java isUseScriptCache），默认 true
    pub use_script_cache: bool,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            mode: "standalone".to_string(),
            host: "localhost".to_string(),
            port: 6379,
            nodes: Vec::new(),
            sentinel_service_name: "mymaster".to_string(),
            sentinel_username: None,
            sentinel_password: None,
            username: None,
            password: None,
            db: 0,
            command_timeout_secs: 0,
            connect_timeout_secs: 5,
            max_command_attempts: 3,
            max_redirections: 5,
            reconnect_max_attempts: 0,
            reconnect_min_delay_ms: 1,
            reconnect_max_delay_ms: 30_000,
            reconnect_multiplier: 2,
            pool_size: 5,
            lock_watchdog_timeout: 30,
            subscription_timeout: 7_500,
            command_timeout_ms: 3_000,
            retry_attempts: 4,
            retry_delay_base_ms: 1_000,
            retry_delay_max_ms: 2_000,
            sharded_subscription_mode: "auto".to_string(),
            read_mode: "slave".to_string(),
            use_script_cache: true,
        }
    }
}

// ============================================================
// RedissonConfig — 程序内部使用，字段类型明确（枚举代替字符串）
// ============================================================

#[derive(Clone)]
pub struct RedissonConfig {
    // ── fred::Config ──
    pub mode: ServerMode,
    pub username: Option<String>,
    pub password: Option<String>,

    // ── fred::PerformanceConfig ──
    /// 对应 Java Config.timeout，命令超时；同时用作 batch 无配置时的默认超时
    pub command_timeout: Duration,

    // ── fred::ConnectionConfig ──
    pub connect_timeout: Duration,
    pub max_command_attempts: u32,
    pub max_redirections: u32,

    // ── fred::ReconnectPolicy ──
    pub reconnect_max_attempts: u32,
    pub reconnect_min_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub reconnect_multiplier: u32,

    // ── fred::Pool / client count ──
    pub pool_size: usize,

    // ── Redisson 行为配置 ──
    /// 对应 Java Config.lockWatchdogTimeout
    pub lock_watchdog_timeout: Duration,
    /// 对应 Java Config.subscriptionTimeout
    pub subscription_timeout: Duration,
    pub retry_attempts: u32,
    /// 对应 Java Config.retryDelay
    pub retry_delay: Duration,

    // ── Pub/Sub ──
    pub sharded_subscription_mode: ShardedSubscriptionMode,

    // ── 读策略 ──
    /// 对应 Java BaseMasterSlaveServersConfig.readMode，默认 ReadMode.SLAVE
    pub read_mode: ReadMode,

    // ── Script 缓存 ──
    /// 对应 Java isUseScriptCache，是否启用 EVALSHA 脚本缓存
    pub use_script_cache: bool,
}

impl TryFrom<RedisConfig> for RedissonConfig {
    type Error = anyhow::Error;

    fn try_from(c: RedisConfig) -> Result<Self> {
        let mode = match c.mode.to_lowercase().as_str() {
            "standalone" => ServerMode::Standalone {
                server: RedisNode {
                    host: c.host.clone(),
                    port: c.port,
                },
                db: c.db,
            },
            "cluster" => {
                anyhow::ensure!(
                    !c.nodes.is_empty(),
                    "Cluster mode requires at least one node in 'nodes'"
                );
                ServerMode::Cluster {
                    nodes: c.nodes.clone(),
                }
            }
            "sentinel" => {
                anyhow::ensure!(
                    !c.nodes.is_empty(),
                    "Sentinel mode requires at least one node in 'nodes'"
                );
                ServerMode::Sentinel {
                    sentinels: c.nodes.clone(),
                    service_name: c.sentinel_service_name.clone(),
                    username: c.sentinel_username.clone(),
                    password: c.sentinel_password.clone(),
                    db: c.db,
                }
            }
            other => anyhow::bail!(
                "Invalid mode '{}'. Expected: standalone, cluster, sentinel",
                other
            ),
        };
        let sharded_subscription_mode = match c.sharded_subscription_mode.to_lowercase().as_str() {
            "auto" => ShardedSubscriptionMode::Auto,
            "on" => ShardedSubscriptionMode::On,
            "off" => ShardedSubscriptionMode::Off,
            other => anyhow::bail!(
                "Invalid sharded_subscription_mode '{}'. Expected: auto, on, off",
                other
            ),
        };
        let read_mode = match c.read_mode.to_lowercase().as_str() {
            "slave" => ReadMode::Slave,
            "master" => ReadMode::Master,
            "master_slave" => ReadMode::MasterSlave,
            other => anyhow::bail!(
                "Invalid read_mode '{}'. Expected: slave, master, master_slave",
                other
            ),
        };
        Ok(Self {
            mode,
            username: c.username,
            password: c.password,
            command_timeout: Duration::from_millis(c.command_timeout_ms),
            connect_timeout: Duration::from_secs(c.connect_timeout_secs),
            max_command_attempts: c.max_command_attempts,
            max_redirections: c.max_redirections,
            reconnect_max_attempts: c.reconnect_max_attempts,
            reconnect_min_delay: Duration::from_millis(c.reconnect_min_delay_ms as u64),
            reconnect_max_delay: Duration::from_millis(c.reconnect_max_delay_ms as u64),
            reconnect_multiplier: c.reconnect_multiplier,
            pool_size: c.pool_size,
            lock_watchdog_timeout: Duration::from_secs(c.lock_watchdog_timeout),
            subscription_timeout: Duration::from_millis(c.subscription_timeout),
            retry_attempts: c.retry_attempts,
            retry_delay: Duration::from_millis(c.retry_delay_base_ms),
            sharded_subscription_mode,
            read_mode,
            use_script_cache: c.use_script_cache,
        })
    }
}

// ============================================================
// fred 配置构建函数（接收 RedissonConfig）
// ============================================================

pub fn build_fred_config(config: &RedissonConfig) -> Result<Config> {
    let (server, database) = match &config.mode {
        ServerMode::Standalone { server: node, db } => (
            ServerConfig::Centralized {
                server: Server::new(&node.host, node.port),
            },
            Some(*db),
        ),
        ServerMode::Cluster { nodes } => (
            ServerConfig::Clustered {
                hosts: nodes.iter().map(|n| Server::new(&n.host, n.port)).collect(),
                policy: ClusterDiscoveryPolicy::default(),
            },
            None,
        ),
        ServerMode::Sentinel {
            sentinels,
            service_name,
            username,
            password,
            db,
        } => (
            ServerConfig::Sentinel {
                hosts: sentinels
                    .iter()
                    .map(|n| Server::new(&n.host, n.port))
                    .collect(),
                service_name: service_name.clone(),
                username: username.clone(),
                password: password.clone(),
            },
            Some(*db),
        ),
    };
    Ok(Config {
        server,
        username: config.username.clone(),
        password: config.password.clone(),
        database,
        ..Default::default()
    })
}

pub(crate) fn build_perf_config(config: &RedissonConfig) -> PerformanceConfig {
    let mut perf = PerformanceConfig::default();
    if config.command_timeout > Duration::ZERO {
        perf.default_command_timeout = config.command_timeout;
    }
    perf
}

pub(crate) fn build_connection_config(config: &RedissonConfig) -> ConnectionConfig {
    let mut conn = ConnectionConfig::default();
    if config.connect_timeout > Duration::ZERO {
        conn.connection_timeout = config.connect_timeout;
    }
    conn.max_command_attempts = config.max_command_attempts;
    conn.max_redirections = config.max_redirections;
    conn
}
