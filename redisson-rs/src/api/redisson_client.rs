use crate::api::rbatch::RBatch;
use crate::api::rbucket::RBucket;
use crate::api::rlock::RLock;
use crate::ext::RedisKey;
use std::sync::Arc;

// ============================================================
// RedissonClient — 对应 Java org.redisson.api.RedissonClient（接口）
// ============================================================

pub trait RedissonClient {
    /// Send + Sync 保证可被多个 task 通过 Arc 共享，对应 Java 多线程共享同一 RLock 对象
    type RLock: RLock + Send + Sync;
    type RBucket: RBucket;
    type RBatch: RBatch;

    /// 对应 Java RedissonClient.getLock(String name)
    fn get_lock<K: RedisKey>(&self, name: K) -> Arc<Self::RLock>;

    /// 对应 Java RedissonClient.getBucket(String name)
    fn get_bucket<K: RedisKey>(&self, name: K) -> Self::RBucket;

    /// 对应 Java RedissonClient.createBatch()
    fn create_batch(&self) -> Self::RBatch;
}
