/// trait_inheritance.rs 的变体：去掉所有 blanket impl
///
/// 策略：
///   - 每个具体类手写全部 trait impl
///   - Deref 链让 accessor 方法体只有一行（&*self / &**self / ...）
///   - 任何 default method 都可以在具体类的 impl 块里直接覆写
///
/// Java 继承链：
///   RedissonObject (abstract)
///     └── RedissonExpirable (abstract)
///           └── RedissonBaseLock (abstract)
///                 └── RedissonLock (concrete)   ← 有自己的字段
///                       └── RedissonFairLock (concrete) ← 有自己的字段

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

// ============================================================
// 第二层：对应 Java RedissonExpirable
// ============================================================

struct RedissonExpirableInner {
    inner: RedissonObjectInner,
}

impl Deref for RedissonExpirableInner {
    type Target = RedissonObjectInner;
    fn deref(&self) -> &RedissonObjectInner {
        &self.inner
    }
}

trait RExpirableLike: RObjectLike {
    fn expirable_inner(&self) -> &RedissonExpirableInner;

    fn expire(&self, seconds: u64) {
        println!("expire key={} seconds={}", self.get_name(), seconds);
    }
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
    fn deref(&self) -> &RedissonExpirableInner {
        &self.inner
    }
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

    fn try_lock_inner_async(&self, thread_id: u64) -> bool;
}

// ============================================================
// 第四层：对应 Java RedissonLock
//
// Java RedissonLock 自己的字段：
//   protected long internalLockLeaseTime
//   protected final LockPubSub pubSub
//   final CommandAsyncExecutor commandExecutor  ← 已在 ObjectInner 层，不重复
// ============================================================

struct LockPubSub;

struct RedissonLockInner {
    inner: RedissonBaseLockInner,
    internal_lock_lease_time: u64,
    pub_sub: LockPubSub,
}

impl Deref for RedissonLockInner {
    type Target = RedissonBaseLockInner;
    fn deref(&self) -> &RedissonBaseLockInner {
        &self.inner
    }
}

trait RedissonLockLike: RedissonBaseLockLike {
    fn redisson_lock_inner(&self) -> &RedissonLockInner;

    fn get_internal_lock_lease_time(&self) -> u64 {
        self.redisson_lock_inner().internal_lock_lease_time
    }
}

// ============================================================
// 具体类：RedissonLock
//
// Deref 链：RedissonLock -> RedissonLockInner -> RedissonBaseLockInner
//                        -> RedissonExpirableInner -> RedissonObjectInner
// ============================================================

struct RedissonLock {
    inner: RedissonLockInner,
}

impl Deref for RedissonLock {
    type Target = RedissonLockInner;
    fn deref(&self) -> &RedissonLockInner {
        &self.inner
    }
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

impl RObjectLike for RedissonLock {
    fn object_inner(&self) -> &RedissonObjectInner { &****self }
}

impl RExpirableLike for RedissonLock {
    fn expirable_inner(&self) -> &RedissonExpirableInner { &***self }
}

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

impl RedissonLockLike for RedissonLock {
    fn redisson_lock_inner(&self) -> &RedissonLockInner { &*self }
}

// ============================================================
// 第五层：对应 Java RedissonFairLock extends RedissonLock
//
// Java RedissonFairLock 自己的字段：
//   private final long threadWaitTime
//   private final String threadsQueueName
//   private final String timeoutSetName
//   private final CommandAsyncExecutor commandExecutor ← 已在 ObjectInner 层，不重复
// ============================================================

struct RedissonFairLockInner {
    inner: RedissonLockInner,
    thread_wait_time: u64,
    threads_queue_name: String,
    timeout_set_name: String,
}

impl Deref for RedissonFairLockInner {
    type Target = RedissonLockInner;
    fn deref(&self) -> &RedissonLockInner {
        &self.inner
    }
}

trait RedissonFairLockLike: RedissonLockLike {
    fn fair_lock_inner(&self) -> &RedissonFairLockInner;

    fn get_threads_queue_name(&self) -> &str {
        &self.fair_lock_inner().threads_queue_name
    }
}

// ============================================================
// 具体类：RedissonFairLock
//
// Deref 链：RedissonFairLock -> RedissonFairLockInner -> RedissonLockInner
//                            -> RedissonBaseLockInner -> RedissonExpirableInner
//                            -> RedissonObjectInner
// ============================================================

struct RedissonFairLock {
    inner: RedissonFairLockInner,
}

impl Deref for RedissonFairLock {
    type Target = RedissonFairLockInner;
    fn deref(&self) -> &RedissonFairLockInner {
        &self.inner
    }
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

impl RObjectLike for RedissonFairLock {
    fn object_inner(&self) -> &RedissonObjectInner { &*****self }
}

impl RExpirableLike for RedissonFairLock {
    fn expirable_inner(&self) -> &RedissonExpirableInner { &****self }

    // 演示覆写：FairLock 覆写了 expireAsync 等相关方法
    fn expire(&self, seconds: u64) {
        println!(
            "RedissonFairLock: expire key={} seconds={} (fair override)",
            self.get_name(),
            seconds
        );
    }
}

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

impl RedissonLockLike for RedissonFairLock {
    fn redisson_lock_inner(&self) -> &RedissonLockInner { &**self }
}

impl RedissonFairLockLike for RedissonFairLock {
    fn fair_lock_inner(&self) -> &RedissonFairLockInner { &*self }
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
    println!("lease_time: {}", fair_lock.get_internal_lock_lease_time()); // 来自 RedissonLockLike
    println!("queue: {}", fair_lock.get_threads_queue_name());            // 来自 RedissonFairLockLike
    fair_lock.expire(30);          // 覆写版本
    fair_lock.schedule_expiration_renewal(42);
    fair_lock.try_lock_inner_async(42);
}
