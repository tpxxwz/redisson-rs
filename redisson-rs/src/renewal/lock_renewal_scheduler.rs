use super::lock_task::LockTask;
use super::renewal_scheduler_trait::RenewalScheduler;
use super::renewal_task::RenewalTask;
use crate::command::command_async_executor::CommandAsyncExecutor;
use std::sync::{Arc, OnceLock};

// ============================================================
// LockRenewalScheduler — 对应 Java org.redisson.renewal.LockRenewalScheduler
// ============================================================

pub struct LockRenewalScheduler<CE: CommandAsyncExecutor> {
    /// 对应 Java LockRenewalScheduler.internalLockLeaseTime
    internal_lock_lease_time: u64,
    /// 对应 Java LockRenewalScheduler.executor — 构造时直接传入
    executor: Arc<CE>,
    /// 对应 Java LockRenewalScheduler.reference (AtomicReference<LockTask>)
    reference: OnceLock<LockTask<CE>>,
}

impl<CE: CommandAsyncExecutor> LockRenewalScheduler<CE> {
    /// 对应 Java LockRenewalScheduler(CommandAsyncExecutor executor)
    pub fn new(executor: Arc<CE>, internal_lock_lease_time: u64) -> Self {
        Self {
            internal_lock_lease_time,
            executor,
            reference: OnceLock::new(),
        }
    }

    /// 惰性获取或创建 LockTask，对应 Java 的 compareAndSet(null, new LockTask(...))
    fn task(&self) -> &LockTask<CE> {
        self.reference
            .get_or_init(|| LockTask::new(self.executor.clone(), self.internal_lock_lease_time))
    }
}

impl<CE: CommandAsyncExecutor> RenewalScheduler for LockRenewalScheduler<CE> {
    fn internal_lock_lease_time(&self) -> u64 {
        self.internal_lock_lease_time
    }

    fn renew_lock(&self, key: String, lock_name: String, thread_id: String) {
        self.task().add(key, lock_name, thread_id);
    }

    fn cancel_lock_renewal(&self, key: &str, thread_id: Option<&str>) {
        if let Some(task) = self.reference.get() {
            task.cancel_expiration_renewal(key, thread_id);
        }
    }

    fn shutdown(&self) {
        if let Some(task) = self.reference.get() {
            task.shutdown();
        }
    }
}
