use crate::api::redisson_client::RedissonClient;
use crate::api::rexpirable_async::RExpirableAsync;
use crate::api::rlock::RLock;
use crate::config::{RedisConfig, RedissonConfig};
use crate::ext::RedisKey;
use crate::redisson::{Redisson, init};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ============================================================
// 测试工具
// ============================================================

fn test_config() -> RedisConfig {
    RedisConfig {
        host: "localhost".to_string(),
        port: 6379,
        password: "wzzst310".to_string(),
        // watchdog 设短一点方便测试
        lock_watchdog_timeout: 5,
        ..Default::default()
    }
}

async fn make_runtime() -> Arc<Redisson> {
    let config = RedissonConfig::try_from(test_config()).expect("invalid config");
    init(config).await.expect("Redis init failed")
}

/// 测试专用 key，自动加 test: 前缀隔离
struct TestKey(&'static str);

impl RedisKey for TestKey {
    fn key(&self) -> String {
        format!("test:lock:{}", self.0)
    }
}

// ============================================================
// 基础: lock / unlock
// ============================================================

#[tokio::test]
async fn
test_basic_lock_unlock() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("basic"));

    lock.lock().await.expect("lock failed");
    assert!(lock.is_locked().await.unwrap(), "should be locked");

    lock.unlock().await.expect("unlock failed");
    assert!(!lock.is_locked().await.unwrap(), "should be unlocked");
}

// ============================================================
// 可重入: 同一 task 可连续 lock 两次，需 unlock 两次才释放
// ============================================================

#[tokio::test]
async fn test_reentrant_lock() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("reentrant"));

    lock.lock().await.unwrap();
    lock.lock().await.unwrap(); // 同一 task 可重入

    assert_eq!(lock.get_hold_count().await.unwrap(), 2);

    lock.unlock().await.unwrap();
    assert_eq!(
        lock.get_hold_count().await.unwrap(),
        1,
        "still held after first unlock"
    );

    lock.unlock().await.unwrap();
    assert!(
        !lock.is_locked().await.unwrap(),
        "released after second unlock"
    );
}

// ============================================================
// 并发: task B 阻塞直到 task A 释放
// ============================================================

#[tokio::test]
async fn test_concurrent_lock_ordering() {
    let rt = make_runtime().await;
    let counter = Arc::new(AtomicU32::new(0));

    // task A: 拿锁 → 写 1 → sleep → 写 2 → 释放
    let rt_a = rt.clone();
    let counter_a = counter.clone();
    let task_a = tokio::spawn(async move {
        let lock = rt_a.get_lock(TestKey("concurrent"));
        lock.lock().await.unwrap();
        counter_a.store(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(200)).await;
        counter_a.store(2, Ordering::SeqCst);
        lock.unlock().await.unwrap();
    });

    // 等 task A 拿到锁
    tokio::time::sleep(Duration::from_millis(50)).await;

    // task B: 必须等 task A 释放后才能拿到锁
    let rt_b = rt.clone();
    let counter_b = counter.clone();
    let task_b = tokio::spawn(async move {
        let lock = rt_b.get_lock(TestKey("concurrent"));
        lock.lock().await.unwrap();
        // 拿到锁时 task A 已写 2
        assert_eq!(
            counter_b.load(Ordering::SeqCst),
            2,
            "task B should see counter=2 (task A completed)"
        );
        lock.unlock().await.unwrap();
    });

    task_a.await.unwrap();
    task_b.await.unwrap();
}

// ============================================================
// is_held_by_current_thread: 当前 task 持有时返回 true
// ============================================================

#[tokio::test]
async fn test_is_held_by_current_thread() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("held_by_current"));

    assert!(
        !lock.is_held_by_current_thread().await.unwrap(),
        "should not be held before lock"
    );

    lock.lock().await.unwrap();
    assert!(
        lock.is_held_by_current_thread().await.unwrap(),
        "should be held after lock"
    );

    // 另一个 task 拿同一把锁，is_held_by_current_thread 对它应返回 false
    let rt2 = rt.clone();
    let held_by_other = tokio::spawn(async move {
        let other_lock = rt2.get_lock(TestKey("held_by_current"));
        other_lock.is_held_by_current_thread().await.unwrap()
    })
    .await
    .unwrap();
    assert!(!held_by_other, "other task should not see itself as holder");

    lock.unlock().await.unwrap();
    assert!(
        !lock.is_held_by_current_thread().await.unwrap(),
        "should not be held after unlock"
    );
}

// ============================================================
// force_unlock: 强制释放（不管持有者）
// ============================================================

#[tokio::test]
async fn test_force_unlock() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("force_unlock"));

    lock.lock().await.unwrap();
    assert!(lock.is_locked().await.unwrap());

    let released = lock.force_unlock().await.unwrap();
    assert!(released, "force_unlock should return true when key existed");
    assert!(
        !lock.is_locked().await.unwrap(),
        "force_unlock should release"
    );

    // 对不存在的锁调用 force_unlock 应返回 false
    let released_again = lock.force_unlock().await.unwrap();
    assert!(
        !released_again,
        "force_unlock on non-existent key should return false"
    );
}

#[tokio::test]
async fn test_force_unlock_reentrant() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("force_unlock_reentrant"));

    // 重入加锁 3 次，hold_count = 3
    lock.lock().await.unwrap();
    lock.lock().await.unwrap();
    lock.lock().await.unwrap();
    assert_eq!(lock.get_hold_count().await.unwrap(), 3);

    // force_unlock 应一次性清除，无论重入多少次
    let released = lock.force_unlock().await.unwrap();
    assert!(released, "force_unlock should return true");
    assert!(
        !lock.is_locked().await.unwrap(),
        "force_unlock should release even with hold_count=3"
    );

    // 释放后其他人应能立刻拿到锁
    let other = rt.get_lock(TestKey("force_unlock_reentrant"));
    assert!(
        other.try_lock().await.unwrap(),
        "another lock should succeed after force_unlock"
    );
    other.unlock().await.unwrap();
}

#[tokio::test]
async fn test_force_unlock_notifies_waiters() {
    let rt = make_runtime().await;

    // task A 持锁
    let lock_a = rt.get_lock(TestKey("force_unlock_notify"));
    lock_a.lock().await.unwrap();

    // task B 等待获取锁
    let rt_b = rt.clone();
    let task_b = tokio::spawn(async move {
        let lock_b = rt_b.get_lock(TestKey("force_unlock_notify"));
        let start = std::time::Instant::now();
        lock_b.lock().await.unwrap();
        let elapsed = start.elapsed();
        lock_b.unlock().await.unwrap();
        elapsed
    });

    // 稍等确保 task B 开始等待，然后 force_unlock
    tokio::time::sleep(Duration::from_millis(100)).await;
    lock_a.force_unlock().await.unwrap();

    // task B 应在远少于 watchdog TTL (5s) 的时间内被唤醒
    let elapsed = tokio::time::timeout(Duration::from_secs(3), task_b)
        .await
        .expect("task B should be notified quickly after force_unlock")
        .unwrap();
    assert!(
        elapsed < Duration::from_secs(3),
        "task B waited too long: {:?}",
        elapsed
    );
}

// ============================================================
// lock_with_lease: 固定 TTL，不续约，到期自动失效
// ============================================================

#[tokio::test]
async fn test_lock_with_lease_expires() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("lock_with_lease"));

    // 500ms 固定 TTL，不启动 watchdog
    lock.lock_with_lease(500).await.unwrap();
    assert!(lock.is_locked().await.unwrap(), "should be locked");

    // 等超过 TTL，锁应自然过期
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !lock.is_locked().await.unwrap(),
        "lock should have expired without watchdog"
    );
}

// ============================================================
// lock_interruptibly: cancel token 触发时应中止等待
// ============================================================

#[tokio::test]
async fn test_lock_interruptibly_cancelled() {
    use tokio_util::sync::CancellationToken;

    let rt = make_runtime().await;

    // task A 持锁
    let lock_a = rt.get_lock(TestKey("interruptibly"));
    lock_a.lock().await.unwrap();

    // task B 用 interruptibly 等待，准备好 cancel token
    let rt_b = rt.clone();
    let cancel = CancellationToken::new();
    let cancel_b = cancel.clone();
    let task_b = tokio::spawn(async move {
        let lock_b = rt_b.get_lock(TestKey("interruptibly"));
        lock_b.lock_interruptibly(cancel_b).await
    });

    // 稍等确保 task B 进入等待，再触发 cancel
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();

    // task B 应快速返回 Err（被中断），而非一直阻塞
    let result = tokio::time::timeout(Duration::from_secs(2), task_b)
        .await
        .expect("task B should return after cancel")
        .unwrap();
    assert!(
        result.is_err(),
        "lock_interruptibly should return Err when cancelled"
    );

    lock_a.unlock().await.unwrap();
}

// ============================================================
// try_lock_with_wait / try_lock_with_wait_and_lease: 超时前获得锁 / 超时返回 false
// ============================================================

#[tokio::test]
async fn test_try_lock_with_wait_success() {
    let rt = make_runtime().await;

    // task A 持锁 100ms 后释放
    let lock_a = rt.get_lock(TestKey("try_timeout_success"));
    lock_a.lock().await.unwrap();

    let rt_b = rt.clone();
    let task_b = tokio::spawn(async move {
        let lock_b = rt_b.get_lock(TestKey("try_timeout_success"));
        // 500ms 内等到锁
        lock_b.try_lock_with_wait(500).await.unwrap()
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    lock_a.unlock().await.unwrap();

    let acquired = tokio::time::timeout(Duration::from_secs(2), task_b)
        .await
        .expect("task B should finish in time")
        .unwrap();
    assert!(
        acquired,
        "try_lock_with_wait should succeed after lock release"
    );
}

#[tokio::test]
async fn test_try_lock_with_wait_expired() {
    let rt = make_runtime().await;

    // task A 持锁，不释放
    let lock_a = rt.get_lock(TestKey("try_timeout_expired"));
    lock_a.lock().await.unwrap();

    let rt_b = rt.clone();
    let task_b = tokio::spawn(async move {
        let lock_b = rt_b.get_lock(TestKey("try_timeout_expired"));
        // 只等 150ms，锁不会释放，应返回 false
        lock_b.try_lock_with_wait(150).await.unwrap()
    });

    let acquired = tokio::time::timeout(Duration::from_secs(2), task_b)
        .await
        .expect("task B should finish in time")
        .unwrap();
    assert!(
        !acquired,
        "try_lock_with_wait should return false on timeout"
    );

    lock_a.unlock().await.unwrap();
}

#[tokio::test]
async fn test_try_lock_with_wait_and_lease() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("try_timeout_lease"));

    // 用固定 lease_ms=500，拿到锁后不续约
    let acquired = lock.try_lock_with_wait_and_lease(200, 500).await.unwrap();
    assert!(acquired, "should acquire lock");
    assert!(lock.is_locked().await.unwrap());

    // 等过期
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !lock.is_locked().await.unwrap(),
        "fixed lease should expire without watchdog"
    );
}

// ============================================================
// remain_time_to_live: 拿锁后 TTL 应 > 0
// ============================================================

#[tokio::test]
async fn test_remain_ttl() {
    let rt = make_runtime().await;
    let lock = rt.get_lock(TestKey("ttl"));

    lock.lock().await.unwrap();

    let ttl = lock.remain_time_to_live().await.unwrap();
    assert!(ttl > 0, "TTL should be positive after lock, got {}", ttl);

    lock.unlock().await.unwrap();
}

// ============================================================
// watchdog: 锁在持有期间不应超时过期
// ============================================================

#[tokio::test]
async fn test_watchdog_renews_lock() {
    let rt = make_runtime().await;
    // watchdog_timeout = 5s，renew 间隔 = 5000/3 ≈ 1.6s
    let lock = rt.get_lock(TestKey("watchdog"));

    lock.lock().await.unwrap();

    // 等待超过一个 watchdog 周期（2s > 1.6s），锁应被续期
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(
        lock.is_locked().await.unwrap(),
        "lock should still be held after watchdog renewal"
    );

    let ttl = lock.remain_time_to_live().await.unwrap();
    assert!(ttl > 0, "TTL should be renewed, got {}", ttl);

    lock.unlock().await.unwrap();
}
