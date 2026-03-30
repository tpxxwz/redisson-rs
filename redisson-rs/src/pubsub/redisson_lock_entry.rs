use std::sync::Arc;
use std::time::Duration;
use tokio::task;

// ============================================================
// RedissonLockEntry — 对应 Java org.redisson.pubsub.RedissonLockEntry
// ============================================================

/// 单个锁的 Pub/Sub 订阅条目，管理等待该锁释放的任务列表和唤醒信号。
/// 对应 Java RedissonLockEntry，内部通过 CountDownLatch 控制等待/唤醒。
#[derive(Clone)]
pub struct RedissonLockEntry {
    pub(crate) waiters: Arc<parking_lot::Mutex<Vec<task::Id>>>,
    pub(crate) signal: Arc<tokio::sync::Notify>,
    pub(crate) subscribe_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl RedissonLockEntry {
    pub fn new() -> Self {
        Self {
            waiters: Arc::new(parking_lot::Mutex::new(Vec::new())),
            signal: Arc::new(tokio::sync::Notify::new()),
            subscribe_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn add_waiter(&self, task_id: task::Id) {
        self.waiters.lock().push(task_id);
    }

    pub fn remove_waiter(&self) {
        self.waiters.lock().pop();
    }

    pub fn waiters_count(&self) -> usize {
        self.waiters.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiters.lock().is_empty()
    }

    /// 唤醒一个等待者（对应 Java tryRunListener + latch.release(1)）
    /// 用于普通锁解锁通知 UNLOCK_MESSAGE
    pub fn try_run_listener(&self) {
        self.signal.notify_one();
    }

    /// 唤醒所有等待者（对应 Java tryRunAllListeners + latch.release(n)）
    /// 用于读锁解锁通知 READ_UNLOCK_MESSAGE
    pub fn try_run_all_listeners(&self) {
        self.signal.notify_waiters();
    }

    pub async fn wait(&self) {
        let notified = self.signal.notified();
        tokio::pin!(notified);

        tokio::select! {
            _ = &mut notified => {}
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                tracing::warn!("Lock entry wait timeout after 60s");
            }
        }
    }
}
