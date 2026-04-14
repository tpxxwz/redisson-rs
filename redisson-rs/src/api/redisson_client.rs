use crate::api::rlock::RLock;
use std::sync::Arc;

// ============================================================
// RedissonClient — 对应 Java org.redisson.api.RedissonClient（接口）
// ============================================================

pub trait RedissonClient {
    /// Send + Sync 保证可被多个 task 通过 Arc 共享，对应 Java 多线程共享同一 RLock 对象
    type RLock: RLock + Send + Sync;

    /// 对应 Java RedissonClient.getLock(String name)
    fn get_lock(&self, name: impl Into<String>) -> Arc<Self::RLock>;

}
