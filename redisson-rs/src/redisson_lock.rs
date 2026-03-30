use crate::api::object_encoding::ObjectEncoding;
use crate::api::object_listener::ObjectListener;
use crate::api::rexpirable::RExpirable;
use crate::api::rexpirable_async::RExpirableAsync;
use crate::api::rlock::RLock;
use crate::api::robject_async::RObjectAsync;
use anyhow::Result;
use bytes::Bytes;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::client::protocol::redis_commands as commands;
use crate::ext::RedisKey;
use crate::pubsub::lock_pub_sub::LockPubSub;
use crate::pubsub::redisson_lock_entry::RedissonLockEntry;
use crate::redisson_base_lock::{LockInner, RLockBase, RedissonBaseLock};
use crate::redisson_object::prefix_name;
use fred::prelude::Value;
use fred::types::FromValue;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ============================================================
// 对应 Java Thread.currentThread().getId()
// ============================================================

/// 对应 Java Thread.currentThread().getId()
fn current_thread_id() -> String {
    tokio::task::try_id()
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("{:?}", std::thread::current().id()))
}

// ============================================================
// RedissonLock — 对应 Java org.redisson.RedissonLock
// ============================================================

/// 可重入分布式锁，对应 Java RedissonLock extends RedissonBaseLock。
/// Rust 用组合代替继承：base 对应 RedissonBaseLock 字段；
/// 通过 Deref 访问基类，通过实现 LockInner/RLockBase trait 提供公共方法。
pub struct RedissonLock<CE: CommandAsyncExecutor> {
    pub(crate) base: RedissonBaseLock<CE>,
    /// 对应 Java RedissonLock.internalLockLeaseTime
    pub(crate) internal_lock_lease_time: Arc<AtomicU64>,
    /// 对应 Java RedissonLock.pubSub
    pub(crate) pub_sub: LockPubSub,
}

impl<CE: CommandAsyncExecutor> Deref for RedissonLock<CE> {
    type Target = RedissonBaseLock<CE>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<CE: CommandAsyncExecutor> RedissonLock<CE> {
    /// 对应 Java RedissonLock(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<CE>, key: impl RedisKey) -> Self {
        let base = RedissonBaseLock::new(command_executor, key);
        let service_manager = command_executor.service_manager();
        Self {
            internal_lock_lease_time: Arc::new(AtomicU64::new(
                service_manager
                    .renewal_scheduler()
                    .internal_lock_lease_time(),
            )),
            pub_sub: LockPubSub::new(
                command_executor
                    .connection_manager()
                    .subscribe_service()
                    .clone(),
            ),
            base,
        }
    }

    // ── 内部实现 ──────────────────────────────────────────────────────────

    /// 对应 Java RedissonLock.getChannelName()
    fn get_channel_name(&self) -> String {
        prefix_name("redisson_lock__channel", &self.get_raw_name())
    }

    /// 对应 Java RedissonLock.tryAcquireAsync()
    async fn try_acquire(&self, lease_time: u64, thread_id: &str) -> Result<Option<u64>> {
        // 对应 Java RedissonLock.tryLockInnerAsync() 中的脚本
        let script = "
            if ((redis.call('exists', KEYS[1]) == 0)
                or (redis.call('hexists', KEYS[1], ARGV[2]) == 1)) then
                redis.call('hincrby', KEYS[1], ARGV[2], 1);
                redis.call('pexpire', KEYS[1], ARGV[1]);
                return nil;
            end;
            return redis.call('pttl', KEYS[1]);
        ";
        let lock_name = self.generate_lock_name(thread_id);
        let name = self.get_raw_name();
        self.command_executor
            .eval_write_async::<Option<u64>, _, _, _>(
                name.as_str(),
                crate::client::protocol::redis_command::RedisCommand::new("EVAL"),
                script,
                vec![name.as_str()],
                vec![Value::from(lease_time.to_string()), Value::from(lock_name)],
            )
            .await
    }

    /// 对应 Java RedissonLock.subscribe(long threadId)
    async fn subscribe(&self, _thread_id: &str) -> Result<Arc<RedissonLockEntry>> {
        let subscribe_timeout_ms = self
            .command_executor
            .service_manager()
            .subscribe_timeout_ms();
        let channel_name = self.get_channel_name();
        let fut = self.pub_sub.subscribe(&self.entry_name, &channel_name);
        if subscribe_timeout_ms == 0 {
            fut.await
        } else {
            tokio::time::timeout(Duration::from_millis(subscribe_timeout_ms), fut)
                .await
                .map_err(|_| {
                    anyhow::anyhow!("Subscribe timeout after {}ms", subscribe_timeout_ms)
                })?
        }
    }

    /// 对应 Java RedissonLock 私有方法 lock(long leaseTime, TimeUnit unit, boolean interruptibly)
    async fn lock_inner(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        cancel: Option<&CancellationToken>,
        thread_id: &str,
    ) -> Result<()> {
        if self.try_acquire(lease_time, thread_id).await?.is_none() {
            if use_watchdog {
                self.schedule_expiration_renewal(thread_id);
            } else {
                self.internal_lock_lease_time
                    .store(lease_time, Ordering::Release);
            }
            return Ok(());
        }

        let entry = self.subscribe(thread_id).await?;
        let result = self
            .lock_wait_loop(lease_time, use_watchdog, cancel, &entry, thread_id)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    /// 对应 Java RedissonLock.lock() 内部循环
    async fn lock_wait_loop(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        cancel: Option<&CancellationToken>,
        entry: &Arc<RedissonLockEntry>,
        thread_id: &str,
    ) -> Result<()> {
        loop {
            let Some(ttl) = self.try_acquire(lease_time, thread_id).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(thread_id);
                } else {
                    self.internal_lock_lease_time
                        .store(lease_time, Ordering::Release);
                }
                return Ok(());
            };

            let wait_dur = Duration::from_millis(ttl + 100);
            if let Some(c) = cancel {
                tokio::select! {
                    _ = c.cancelled() => anyhow::bail!("lock_interruptibly: cancelled"),
                    _ = entry.wait() => {}
                    _ = tokio::time::sleep(wait_dur) => {}
                }
            } else {
                tokio::select! {
                    _ = entry.wait() => {}
                    _ = tokio::time::sleep(wait_dur) => {}
                }
            }
        }
    }

    /// 对应 Java RedissonLock.tryLock(long waitTime, ...) 内部循环
    async fn try_lock_wait_loop(
        &self,
        lease_time: u64,
        use_watchdog: bool,
        deadline: tokio::time::Instant,
        entry: &Arc<RedissonLockEntry>,
        thread_id: &str,
    ) -> Result<bool> {
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }

            let Some(ttl) = self.try_acquire(lease_time, thread_id).await? else {
                if use_watchdog {
                    self.schedule_expiration_renewal(thread_id);
                } else {
                    self.internal_lock_lease_time
                        .store(lease_time, Ordering::Release);
                }
                return Ok(true);
            };

            let wait_dur = remaining.min(Duration::from_millis(ttl + 100));
            tokio::select! {
                _ = tokio::time::sleep(remaining) => return Ok(false),
                _ = entry.wait() => {}
                _ = tokio::time::sleep(wait_dur) => {}
            }
        }
    }
}

// ============================================================
// impl LockInner for RedissonLock
// 对应 Java RedissonLock 中覆写/实现的抽象方法
// ============================================================

impl<CE: CommandAsyncExecutor> LockInner for RedissonLock<CE> {
    /// 对应 Java RedissonLock.unlockInnerAsync(long threadId, String requestId, int timeout)
    async fn unlock_inner_async(
        &self,
        thread_id: &str,
        request_id: &str,
        timeout: i32,
    ) -> Result<Option<i64>> {
        // 对应 Java RedissonLock.unlockInnerAsync() 中的脚本
        let script = "
            local val = redis.call('get', KEYS[3]);
            if val ~= false then
                return tonumber(val);
            end;
            if (redis.call('hexists', KEYS[1], ARGV[3]) == 0) then
                return nil;
            end;
            local counter = redis.call('hincrby', KEYS[1], ARGV[3], -1);
            if (counter > 0) then
                redis.call('pexpire', KEYS[1], ARGV[2]);
                redis.call('set', KEYS[3], 0, 'px', ARGV[5]);
                return 0;
            else
                redis.call('del', KEYS[1]);
                redis.call(ARGV[4], KEYS[2], ARGV[1]);
                redis.call('set', KEYS[3], 1, 'px', ARGV[5]);
                return 1;
            end;
        ";
        let lock_name = self.generate_lock_name(thread_id);
        let latch_key = self.get_unlock_latch_name(request_id);
        let lease_time_str = self
            .internal_lock_lease_time
            .load(Ordering::Acquire)
            .to_string();
        let publish_cmd = self
            .command_executor
            .connection_manager()
            .subscribe_service()
            .publish_command();

        let lock_key = self.get_raw_name();
        let channel_key = self.get_channel_name();
        let result: Option<i64> = self
            .command_executor
            .eval_write_async(
                lock_key.as_str(),
                crate::client::protocol::redis_command::RedisCommand::new("EVAL"),
                script,
                vec![
                    lock_key.as_str(),
                    channel_key.as_str(),
                    latch_key.as_str(),
                ],
                vec![
                    Value::from("0"),
                    Value::from(lease_time_str.clone()),
                    Value::from(lock_name.clone()),
                    Value::from(publish_cmd),
                    Value::from(timeout.to_string()),
                ],
            )
            .await?;

        // 异步删除 latch key
        {
            let executor = self.command_executor.clone();
            let latch_key_owned = latch_key.clone();
            tokio::spawn(async move {
                let _ = executor
                    .write_async::<i64, _, _>(&latch_key_owned, commands::DEL, vec![Value::from(latch_key_owned.clone())])
                    .await;
            });
        }

        Ok(result)
    }

    /// 对应 Java RedissonLock.forceUnlockAsync()
    async fn force_unlock_inner_async(&self) -> Result<bool> {
        // 对应 Java RedissonLock.forceUnlockAsync() 中的脚本
        let script = "
            if (redis.call('del', KEYS[1]) == 1) then
                redis.call(ARGV[2], KEYS[2], ARGV[1]);
                return 1;
            else
                return 0;
            end;
        ";
        let publish_cmd = self
            .command_executor
            .connection_manager()
            .subscribe_service()
            .publish_command();
        let lock_key = self.get_raw_name();
        let channel_key = self.get_channel_name();
        let raw: Value = self
            .command_executor
            .eval_write_async(
                lock_key.as_str(),
                commands::EVAL_OBJECT,
                script,
                vec![lock_key.as_str(), channel_key.as_str()],
                vec![Value::from("0"), Value::from(publish_cmd)],
            )
            .await?;
        Ok(i64::from_value(raw)? == 1)
    }

    /// 对应 Java RedissonLock.cancelExpirationRenewal override：
    /// 调用基类后，若锁完全释放则重置 internalLockLeaseTime 回 watchdog timeout
    fn cancel_expiration_renewal(&self, thread_id: Option<&str>, unlock_result: Option<bool>) {
        self.cancel_expiration_renewal_base(thread_id);

        if unlock_result.is_none() || unlock_result == Some(true) {
            let watchdog_timeout = self
                .command_executor
                .service_manager()
                .renewal_scheduler()
                .internal_lock_lease_time();
            self.internal_lock_lease_time
                .store(watchdog_timeout, Ordering::Release);
        }
    }
}

// ============================================================
// impl RLockBase for RedissonLock
// 对应 Java RedissonBaseLock 中 RLock 接口的公共方法
// ============================================================

impl<CE: CommandAsyncExecutor> RLockBase<CE> for RedissonLock<CE> {
    fn command_executor(&self) -> &Arc<CE> {
        &self.base.command_executor
    }

    fn lock_name(&self) -> String {
        self.get_raw_name()
    }

    fn lock_id(&self) -> &str {
        &self.base.id
    }

    fn entry_name(&self) -> &str {
        &self.entry_name
    }
}

// ============================================================
// impl RObjectAsync for RedissonLock（委托给 RedissonBaseLock）
// 方法顺序与 Java RObjectAsync 保持一致
// ============================================================

impl<CE: CommandAsyncExecutor> RObjectAsync for RedissonLock<CE> {
    fn get_name(&self) -> String {
        self.base.get_name()
    }

    // 1. getIdleTimeAsync
    fn get_idle_time_async(&self) -> impl Future<Output = Result<i64>> + Send {
        self.base.get_idle_time_async()
    }

    // 2. getReferenceCountAsync
    fn get_reference_count_async(&self) -> impl Future<Output = Result<i32>> + Send {
        self.base.get_reference_count_async()
    }

    // 3. getAccessFrequencyAsync
    fn get_access_frequency_async(&self) -> impl Future<Output = Result<i32>> + Send {
        self.base.get_access_frequency_async()
    }

    // 4. getInternalEncodingAsync
    fn get_internal_encoding_async(&self) -> impl Future<Output = Result<ObjectEncoding>> + Send {
        self.base.get_internal_encoding_async()
    }

    // 5. sizeInMemoryAsync
    fn size_in_memory_async(&self) -> impl Future<Output = Result<i64>> + Send {
        self.base.size_in_memory_async()
    }

    // 6. restoreAsync(byte[] state)
    fn restore_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_async(state)
    }

    // 7. restoreAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_with_ttl_async(state, time_to_live)
    }

    // 8. restoreAndReplaceAsync(byte[] state)
    fn restore_and_replace_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_and_replace_async(state)
    }

    // 9. restoreAndReplaceAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_and_replace_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_and_replace_with_ttl_async(state, time_to_live)
    }

    // 10. dumpAsync
    fn dump_async(&self) -> impl Future<Output = Result<Bytes>> + Send {
        self.base.dump_async()
    }

    // 11. touchAsync
    fn touch_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.touch_async()
    }

    // 12. migrateAsync(String host, int port, int database, long timeout)
    fn migrate_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send {
        self.base.migrate_async(host, port, database, timeout)
    }

    // 13. copyAsync(String host, int port, int database, long timeout)
    fn copy_to_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send {
        self.base.copy_to_async(host, port, database, timeout)
    }

    // 14. copyAsync(String destination)
    fn copy_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_async(destination)
    }

    // 15. copyAsync(String destination, int database)
    fn copy_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_to_database_async(destination, database)
    }

    // 16. copyAndReplaceAsync(String destination)
    fn copy_and_replace_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_and_replace_async(destination)
    }

    // 17. copyAndReplaceAsync(String destination, int database)
    fn copy_and_replace_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_and_replace_to_database_async(destination, database)
    }

    // 18. moveAsync(int database)
    fn move_async(&self, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.move_async(database)
    }

    // 19. deleteAsync
    fn delete_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.delete_async()
    }

    // 20. unlinkAsync
    fn unlink_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.unlink_async()
    }

    // 21. renameAsync(String newName)
    fn rename_async(&self, new_name: &str) -> impl Future<Output = Result<()>> + Send {
        self.base.rename_async(new_name)
    }

    // 22. renamenxAsync(String newName)
    fn renamenx_async(&self, new_name: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.renamenx_async(new_name)
    }

    // 23. isExistsAsync
    fn is_exists_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.is_exists_async()
    }

    // 24. addListenerAsync(ObjectListener listener)
    fn add_listener_async(&self, listener: Box<dyn ObjectListener + Send + Sync>) -> impl Future<Output = Result<i32>> + Send {
        self.base.add_listener_async(listener)
    }

    // 25. removeListenerAsync(int listenerId)
    fn remove_listener_async(&self, listener_id: i32) -> impl Future<Output = Result<()>> + Send {
        self.base.remove_listener_async(listener_id)
    }
}

// ============================================================
// impl RExpirableAsync for RedissonLock（委托给 RedissonBaseLock）
// ============================================================

impl<CE: CommandAsyncExecutor> RExpirableAsync for RedissonLock<CE> {
    async fn expire(&self, duration: Duration) -> Result<bool> {
        self.base.expire(duration).await
    }

    async fn expire_at(&self, timestamp_millis: u64) -> Result<bool> {
        self.base.expire_at(timestamp_millis).await
    }

    async fn expire_if_set(&self, duration: Duration) -> Result<bool> {
        self.base.expire_if_set(duration).await
    }

    async fn expire_if_not_set(&self, duration: Duration) -> Result<bool> {
        self.base.expire_if_not_set(duration).await
    }

    async fn expire_if_greater(&self, duration: Duration) -> Result<bool> {
        self.base.expire_if_greater(duration).await
    }

    async fn expire_if_less(&self, duration: Duration) -> Result<bool> {
        self.base.expire_if_less(duration).await
    }

    async fn clear_expire(&self) -> Result<bool> {
        self.base.clear_expire().await
    }

    async fn remain_time_to_live(&self) -> Result<i64> {
        self.base.remain_time_to_live().await
    }

    async fn get_expire_time(&self) -> Result<i64> {
        self.base.get_expire_time().await
    }
}

// ============================================================
// impl RExpirable for RedissonLock (空实现，继承自 RExpirableAsync)
// ============================================================

impl<CE: CommandAsyncExecutor> RExpirable for RedissonLock<CE> {}

// ============================================================
// impl RLock for RedissonLock — 对应 Java RedissonLock implements RLock
// 方法顺序：与 Java RedissonLock 保持一致
// ============================================================

impl<CE: CommandAsyncExecutor> RLock for RedissonLock<CE> {
    // ── lock 系列 ───────────────────────────────────────────────────────

    /// 对应 Java Lock: void lock()
    async fn lock(&self) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(
            self.internal_lock_lease_time.load(Ordering::Acquire),
            true,
            None,
            &thread_id,
        )
        .await
    }

    /// 对应 Java RLock: void lock(long leaseTime, TimeUnit unit)
    async fn lock_with_lease(&self, lease_ms: u64) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(lease_ms, false, None, &thread_id).await
    }

    /// 对应 Java Lock: void lockInterruptibly()
    async fn lock_interruptibly(&self, cancel: CancellationToken) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(
            self.internal_lock_lease_time.load(Ordering::Acquire),
            true,
            Some(&cancel),
            &thread_id,
        )
        .await
    }

    /// 对应 Java RLock: void lockInterruptibly(long leaseTime, TimeUnit unit)
    async fn lock_interruptibly_with_lease(
        &self,
        lease_ms: u64,
        cancel: CancellationToken,
    ) -> Result<()> {
        let thread_id = current_thread_id();
        self.lock_inner(lease_ms, false, Some(&cancel), &thread_id)
            .await
    }

    // ── tryLock 系列 ────────────────────────────────────────────────────

    /// 对应 Java Lock: boolean tryLock()
    async fn try_lock(&self) -> Result<bool> {
        let thread_id = current_thread_id();
        if self
            .try_acquire(
                self.internal_lock_lease_time.load(Ordering::Acquire),
                &thread_id,
            )
            .await?
            .is_none()
        {
            self.schedule_expiration_renewal(&thread_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 对应 Java RLock: boolean tryLock(long waitTime, long leaseTime, TimeUnit unit)
    async fn try_lock_with_wait_and_lease(&self, wait_ms: u64, lease_ms: u64) -> Result<bool> {
        let thread_id = current_thread_id();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        if self.try_acquire(lease_ms, &thread_id).await?.is_none() {
            self.internal_lock_lease_time
                .store(lease_ms, Ordering::Release);
            return Ok(true);
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }

        let entry = self.subscribe(&thread_id).await?;
        let result = self
            .try_lock_wait_loop(lease_ms, false, deadline, &entry, &thread_id)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    /// 对应 Java Lock: boolean tryLock(long time, TimeUnit unit)
    async fn try_lock_with_wait(&self, wait_ms: u64) -> Result<bool> {
        let thread_id = current_thread_id();
        let lease_time = self.internal_lock_lease_time.load(Ordering::Acquire);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        if self.try_acquire(lease_time, &thread_id).await?.is_none() {
            self.schedule_expiration_renewal(&thread_id);
            return Ok(true);
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }

        let entry = self.subscribe(&thread_id).await?;
        let result = self
            .try_lock_wait_loop(lease_time, true, deadline, &entry, &thread_id)
            .await;
        let _ = self
            .pub_sub
            .unsubscribe(&self.entry_name, &self.get_channel_name())
            .await;
        result
    }

    // ── unlock ───────────────────────────────────────────────────────────

    /// 对应 Java Lock: void unlock()
    async fn unlock(&self) -> Result<()> {
        let thread_id = current_thread_id();
        <Self as RLockBase<CE>>::unlock_async(self, &thread_id).await
    }

    // ── 其他 RLock 方法 ──────────────────────────────────────────────────

    /// 对应 Java RLock: boolean forceUnlock()
    async fn force_unlock(&self) -> Result<bool> {
        <Self as RLockBase<CE>>::force_unlock(self).await
    }

    /// 对应 Java RLock: boolean isLocked()
    async fn is_locked(&self) -> Result<bool> {
        <Self as RLockBase<CE>>::is_locked(self).await
    }

    /// 对应 Java RLock: boolean isHeldByThread(long threadId)
    async fn is_held_by_thread(&self, thread_id: u64) -> Result<bool> {
        <Self as RLockBase<CE>>::is_held_by_thread(self, &thread_id.to_string()).await
    }

    /// 对应 Java RLock: boolean isHeldByCurrentThread()
    async fn is_held_by_current_thread(&self) -> Result<bool> {
        let thread_id = current_thread_id();
        <Self as RLockBase<CE>>::is_held_by_current_thread(self, &thread_id).await
    }

    /// 对应 Java RLock: int getHoldCount()
    async fn get_hold_count(&self) -> Result<i64> {
        let thread_id = current_thread_id();
        <Self as RLockBase<CE>>::get_hold_count(self, &thread_id).await
    }
}
