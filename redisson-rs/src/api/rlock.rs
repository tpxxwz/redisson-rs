use crate::api::rexpirable::RExpirable;
use anyhow::Result;
use std::future::Future;
use tokio_util::sync::CancellationToken;

// ============================================================
// RLock — 对应 Java org.redisson.api.RLock（接口）
// ============================================================

/// 可重入分布式锁接口，对应 Java RLock extends Lock, RExpirable, RLockAsync。
/// 实现类：RedissonLock
///
/// 方法顺序：
/// 1. 先按 java.util.concurrent.locks.Lock 接口顺序
/// 2. 再按 RLock 接口扩展的方法顺序
pub trait RLock: RExpirable {
    // ── lock 系列（对应 Java Lock + RLock 扩展）─────────────────────────

    /// Acquires the lock. Waits if necessary until lock became available.
    /// 对应 Java Lock: void lock()
    fn lock(&self) -> impl Future<Output = Result<()>> + Send;

    /// Acquires the lock with defined leaseTime. Waits if necessary until lock became available.
    /// Lock will be released automatically after defined leaseTime interval.
    /// 对应 Java RLock: void lock(long leaseTime, TimeUnit unit)
    fn lock_with_lease(&self, lease_ms: u64) -> impl Future<Output = Result<()>> + Send;

    /// Acquires the lock. Waits if necessary until lock became available.
    /// 对应 Java Lock: void lockInterruptibly()
    fn lock_interruptibly(
        &self,
        cancel: CancellationToken,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Acquires the lock with defined leaseTime. Waits if necessary until lock became available.
    /// Lock will be released automatically after defined leaseTime interval.
    /// 对应 Java RLock: void lockInterruptibly(long leaseTime, TimeUnit unit)
    fn lock_interruptibly_with_lease(
        &self,
        lease_ms: u64,
        cancel: CancellationToken,
    ) -> impl Future<Output = Result<()>> + Send;

    // ── tryLock 系列 ────────────────────────────────────────────────────

    /// Acquires the lock only if it is free at the time of invocation.
    /// 对应 Java Lock: boolean tryLock()
    fn try_lock(&self) -> impl Future<Output = Result<bool>> + Send;

    /// Tries to acquire the lock with defined leaseTime.
    /// Waits up to defined waitTime if necessary until the lock became available.
    /// Lock will be released automatically after defined leaseTime interval.
    /// 对应 Java RLock: boolean tryLock(long waitTime, long leaseTime, TimeUnit unit)
    fn try_lock_with_wait_and_lease(
        &self,
        wait_ms: u64,
        lease_ms: u64,
    ) -> impl Future<Output = Result<bool>> + Send;

    /// Acquires the lock if it is free within the given waiting time.
    /// 对应 Java Lock: boolean tryLock(long time, TimeUnit unit)
    fn try_lock_with_wait(&self, wait_ms: u64) -> impl Future<Output = Result<bool>> + Send;

    // ── unlock ───────────────────────────────────────────────────────────

    /// Releases the lock.
    /// 对应 Java Lock: void unlock()
    fn unlock(&self) -> impl Future<Output = Result<()>> + Send;

    // ── 其他 RLock 方法 ──────────────────────────────────────────────────

    /// Unlocks the lock independently of its state
    /// 对应 Java RLock: boolean forceUnlock()
    fn force_unlock(&self) -> impl Future<Output = Result<bool>> + Send;

    /// Checks if the lock locked by any thread
    /// 对应 Java RLock: boolean isLocked()
    fn is_locked(&self) -> impl Future<Output = Result<bool>> + Send;

    /// Checks if the lock is held by thread with defined threadId
    /// 对应 Java RLock: boolean isHeldByThread(long threadId)
    fn is_held_by_thread(&self, thread_id: u64) -> impl Future<Output = Result<bool>> + Send;

    /// Checks if this lock is held by the current thread
    /// 对应 Java RLock: boolean isHeldByCurrentThread()
    fn is_held_by_current_thread(&self) -> impl Future<Output = Result<bool>> + Send;

    /// Number of holds on this lock by the current thread
    /// 对应 Java RLock: int getHoldCount()
    fn get_hold_count(&self) -> impl Future<Output = Result<i64>> + Send;
}
