// ============================================================
// RenewalTask — 对应 Java org.redisson.renewal.RenewalTask（抽象类）
// ============================================================

/// 续约任务接口，对应 Java abstract class RenewalTask implements TimerTask。
pub trait RenewalTask: Send + Sync {
    /// 注册锁续约，对应 Java RenewalTask.add(rawName, lockName, threadId, entry)
    fn add(&self, name: String, lock_name: String, thread_id: String);

    /// 注销锁续约，对应 Java RenewalTask.cancelExpirationRenewal(name, threadId)
    /// thread_id 为 None 时对应 Java 传 null（forceUnlock），直接移除整个 entry
    fn cancel_expiration_renewal(&self, name: &str, thread_id: Option<&str>);

    /// 停止续约循环
    fn shutdown(&self);
}
