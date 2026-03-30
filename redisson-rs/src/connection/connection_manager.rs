use super::service_manager::ServiceManager;
use crate::config::RedissonConfig;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use async_trait::async_trait;
use std::sync::Arc;
// ============================================================
// ConnectionManager — 对应 Java org.redisson.connection.ConnectionManager（接口）
// ============================================================

/// 连接管理器接口，对应 Java org.redisson.connection.ConnectionManager。
/// 实现类：FredConnectionManager
///
/// Java 接口中与 Netty 连接模型强绑定的方法（MasterSlaveEntry、RedisClient、ByteBuf 等）
/// 在 Rust/fred 中由 fred Pool 内部管理，无法也无需移植，故不在此 trait 中定义。
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// 对应 Java ConnectionManager.getSubscribeService()
    fn subscribe_service(&self) -> &Arc<PublishSubscribeService>;
    /// 对应 Java ConnectionManager.getServiceManager()
    fn service_manager(&self) -> &Arc<ServiceManager>;
    /// 对应 Java ConnectionManager.shutdown()
    /// 停止 watchdog 续约，向所有连接发送 QUIT 命令
    async fn shutdown(&self);

    /// 对应 Java ConnectionManager.calcSlot(String/ByteBuf/byte[] key)
    /// 计算 key 的 cluster hash slot（0-16383）
    /// &[u8] 统一覆盖 Java 三个重载，调用方 .as_bytes() 即可
    fn calc_slot(&self, key: &[u8]) -> u16;
    /// 对应 Java connectionManager.getServiceManager().getCfg()
    fn config(&self) -> &Arc<RedissonConfig>;
}

