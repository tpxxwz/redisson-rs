// ============================================================
// ReadMode — 对应 Java org.redisson.config.ReadMode
// ============================================================

/// 对应 Java org.redisson.config.ReadMode。
/// 存储在 BaseMasterSlaveServersConfig，默认值为 SLAVE。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadMode {
    /// 对应 Java ReadMode.SLAVE：从 slave 读，无 slave 时降级 master
    Slave,
    /// 对应 Java ReadMode.MASTER：始终从 master 读
    Master,
    /// 对应 Java ReadMode.MASTER_SLAVE：master + slave 之间负载均衡
    MasterSlave,
}

impl Default for ReadMode {
    /// 对应 Java BaseMasterSlaveServersConfig 默认值 ReadMode.SLAVE
    fn default() -> Self {
        ReadMode::Slave
    }
}
