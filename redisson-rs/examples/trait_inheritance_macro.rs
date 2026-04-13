/// trait_inheritance_no_blanket.rs 的宏优化版
///
/// 用 per-trait 宏消除 accessor 一行代码的重复：
///   - 不需要覆写的 trait → 调一行宏
///   - 需要覆写的 trait   → 跳过宏，手写完整 impl 块（可加覆写方法）
///   - 有抽象方法的 trait  → 始终手写（RedissonBaseLockLike::try_lock_inner_async）

use std::ops::Deref;

// ============================================================
// 第一层：对应 Java RedissonObject
// ============================================================

struct CommandAsyncExecutor;

struct RedissonObjectInner {
    command_executor: CommandAsyncExecutor,
    name: String,
    codec: String,
}

trait RObjectLike {
    fn object_inner(&self) -> &RedissonObjectInner;

    fn get_name(&self) -> &str {
        &self.object_inner().name
    }
}

macro_rules! impl_robject {
    ($T:ty, $($star:tt)*) => {
        impl RObjectLike for $T {
            fn object_inner(&self) -> &RedissonObjectInner { & $($star)* self }
        }
    };
}

// ============================================================
// 第二层：对应 Java RedissonExpirable
// ============================================================

struct RedissonExpirableInner {
    inner: RedissonObjectInner,
}

impl Deref for RedissonExpirableInner {
    type Target = RedissonObjectInner;
    fn deref(&self) -> &RedissonObjectInner { &self.inner }
}

trait RExpirableLike: RObjectLike {
    fn expirable_inner(&self) -> &RedissonExpirableInner;

    fn expire(&self, seconds: u64) {
        println!("expire key={} seconds={}", self.get_name(), seconds);
    }
}

macro_rules! impl_rexpirable {
    ($T:ty, $($star:tt)*) => {
        impl RExpirableLike for $T {
            fn expirable_inner(&self) -> &RedissonExpirableInner { & $($star)* self }
        }
    };
}

// ============================================================
// 第三层：对应 Java RedissonBaseLock
// ============================================================

struct LockRenewalScheduler;

struct RedissonBaseLockInner {
    inner: RedissonExpirableInner,
    id: String,
    entry_name: String,
    renewal_scheduler: LockRenewalScheduler,
}

impl Deref for RedissonBaseLockInner {
    type Target = RedissonExpirableInner;
    fn deref(&self) -> &RedissonExpirableInner { &self.inner }
}

trait RedissonBaseLockLike: RExpirableLike {
    fn lock_inner(&self) -> &RedissonBaseLockInner;

    fn get_entry_name(&self) -> &str {
        &self.lock_inner().entry_name
    }

    fn get_lock_name(&self, thread_id: u64) -> String {
        format!("{}:{}", self.lock_inner().id, thread_id)
    }

    fn schedule_expiration_renewal(&self, thread_id: u64) {
        println!(
            "renewal scheduled: lock={} thread={}",
            self.get_entry_name(),
            thread_id
        );
    }

    // 抽象方法，无 default，始终手写
    fn try_lock_inner_async(&self, thread_id: u64) -> bool;
}

// RedissonBaseLockLike 因含抽象方法，不提供纯 accessor 宏，始终手写

// ============================================================
// 第四层：对应 Java RedissonLock
// ============================================================

struct LockPubSub;

struct RedissonLockInner {
    inner: RedissonBaseLockInner,
    internal_lock_lease_time: u64,
    pub_sub: LockPubSub,
}

impl Deref for RedissonLockInner {
    type Target = RedissonBaseLockInner;
    fn deref(&self) -> &RedissonBaseLockInner { &self.inner }
}

trait RedissonLockLike: RedissonBaseLockLike {
    fn redisson_lock_inner(&self) -> &RedissonLockInner;

    fn get_internal_lock_lease_time(&self) -> u64 {
        self.redisson_lock_inner().internal_lock_lease_time
    }
}

macro_rules! impl_redisson_lock {
    ($T:ty, $($star:tt)*) => {
        impl RedissonLockLike for $T {
            fn redisson_lock_inner(&self) -> &RedissonLockInner { & $($star)* self }
        }
    };
}

// ============================================================
// 第五层：对应 Java RedissonFairLock
// ============================================================

struct RedissonFairLockInner {
    inner: RedissonLockInner,
    thread_wait_time: u64,
    threads_queue_name: String,
    timeout_set_name: String,
}

impl Deref for RedissonFairLockInner {
    type Target = RedissonLockInner;
    fn deref(&self) -> &RedissonLockInner { &self.inner }
}

trait RedissonFairLockLike: RedissonLockLike {
    fn fair_lock_inner(&self) -> &RedissonFairLockInner;

    fn get_threads_queue_name(&self) -> &str {
        &self.fair_lock_inner().threads_queue_name
    }
}

macro_rules! impl_fair_lock {
    ($T:ty, $($star:tt)*) => {
        impl RedissonFairLockLike for $T {
            fn fair_lock_inner(&self) -> &RedissonFairLockInner { & $($star)* self }
        }
    };
}

// ============================================================
// 具体类：RedissonLock
// ============================================================

struct RedissonLock {
    inner: RedissonLockInner,
}

impl Deref for RedissonLock {
    type Target = RedissonLockInner;
    fn deref(&self) -> &RedissonLockInner { &self.inner }
}

impl RedissonLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonLock {
            inner: RedissonLockInner {
                inner: RedissonBaseLockInner {
                    inner: RedissonExpirableInner {
                        inner: RedissonObjectInner {
                            command_executor: CommandAsyncExecutor,
                            name: name.to_string(),
                            codec: "default".to_string(),
                        },
                    },
                    id: id.to_string(),
                    entry_name: format!("{}:{}", id, name),
                    renewal_scheduler: LockRenewalScheduler,
                },
                internal_lock_lease_time: 30_000,
                pub_sub: LockPubSub,
            },
        }
    }
}

impl_robject!(RedissonLock, ****);
impl_rexpirable!(RedissonLock, ***);
impl_redisson_lock!(RedissonLock, *);

// 有抽象方法，手写
impl RedissonBaseLockLike for RedissonLock {
    fn lock_inner(&self) -> &RedissonBaseLockInner { &**self }

    fn try_lock_inner_async(&self, thread_id: u64) -> bool {
        println!(
            "RedissonLock: try lock key={} lock_name={}",
            self.get_name(),
            self.get_lock_name(thread_id)
        );
        true
    }
}

// ============================================================
// 具体类：RedissonFairLock
// ============================================================

struct RedissonFairLock {
    inner: RedissonFairLockInner,
}

impl Deref for RedissonFairLock {
    type Target = RedissonFairLockInner;
    fn deref(&self) -> &RedissonFairLockInner { &self.inner }
}

impl RedissonFairLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonFairLock {
            inner: RedissonFairLockInner {
                inner: RedissonLockInner {
                    inner: RedissonBaseLockInner {
                        inner: RedissonExpirableInner {
                            inner: RedissonObjectInner {
                                command_executor: CommandAsyncExecutor,
                                name: name.to_string(),
                                codec: "default".to_string(),
                            },
                        },
                        id: id.to_string(),
                        entry_name: format!("{}:{}", id, name),
                        renewal_scheduler: LockRenewalScheduler,
                    },
                    internal_lock_lease_time: 30_000,
                    pub_sub: LockPubSub,
                },
                thread_wait_time: 5_000,
                threads_queue_name: format!("redisson_lock_queue:{}", name),
                timeout_set_name: format!("redisson_lock_timeout:{}", name),
            },
        }
    }
}

impl_robject!(RedissonFairLock, *****);
impl_redisson_lock!(RedissonFairLock, **);
impl_fair_lock!(RedissonFairLock, *);

// expire 需要覆写，跳过 impl_rexpirable 宏，手写
impl RExpirableLike for RedissonFairLock {
    fn expirable_inner(&self) -> &RedissonExpirableInner { &****self }

    fn expire(&self, seconds: u64) {
        println!(
            "RedissonFairLock: expire key={} seconds={} (fair override)",
            self.get_name(),
            seconds
        );
    }
}

// 有抽象方法，手写
impl RedissonBaseLockLike for RedissonFairLock {
    fn lock_inner(&self) -> &RedissonBaseLockInner { &***self }

    fn try_lock_inner_async(&self, thread_id: u64) -> bool {
        println!(
            "RedissonFairLock: fair try lock key={} lock_name={} queue={}",
            self.get_name(),
            self.get_lock_name(thread_id),
            self.get_threads_queue_name(),
        );
        true
    }
}

// ============================================================
// 验证
// ============================================================

fn main() {
    let lock = RedissonLock::new("my-lock", "client-1");

    println!("name: {}", lock.get_name());
    println!("lease_time: {}", lock.get_internal_lock_lease_time());
    lock.expire(30);
    println!("entry_name: {}", lock.get_entry_name());
    println!("lock_name: {}", lock.get_lock_name(42));
    lock.schedule_expiration_renewal(42);
    lock.try_lock_inner_async(42);

    println!("---");

    let fair_lock = RedissonFairLock::new("my-fair-lock", "client-1");

    println!("name: {}", fair_lock.get_name());
    println!("lease_time: {}", fair_lock.get_internal_lock_lease_time());
    println!("queue: {}", fair_lock.get_threads_queue_name());
    fair_lock.expire(30);
    fair_lock.schedule_expiration_renewal(42);
    fair_lock.try_lock_inner_async(42);
}
