// ============================================================
// RenewalScheduler — 对应 Java org.redisson.renewal.LockRenewalScheduler（接口抽象）
// ============================================================

/// watchdog 续约调度器接口，object-safe。
/// ServiceManager 通过 Arc<dyn RenewalScheduler> 持有，不感知泛型 CE。
pub trait RenewalScheduler: Send + Sync {
    fn internal_lock_lease_time(&self) -> u64;

    /// 对应 Java LockRenewalScheduler.renewLock(name, threadId, lockName)
    fn renew_lock(&self, key: String, lock_name: String, thread_id: String);

    /// 对应 Java LockRenewalScheduler.cancelLockRenewal(name, threadId)
    /// thread_id 为 None 对应 Java 传 null（forceUnlock），直接移除整个 entry
    fn cancel_lock_renewal(&self, key: &str, thread_id: Option<&str>);

    fn shutdown(&self);
}
