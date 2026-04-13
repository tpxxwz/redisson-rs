/// 演示用 trait + struct 嵌套模拟 Java 多层抽象类继承
///
/// Java 继承链：
///   RedissonObject (abstract)
///     └── RedissonExpirable (abstract)
///           └── RedissonBaseLock (abstract)
///                 ├── RedissonLock (concrete)
///                 │     └── RedissonFairLock (concrete)
///                 └── RedissonSpinLock (concrete)

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
    base: RedissonObjectInner,
}

trait RExpirableLike: RObjectLike {
    fn expirable_inner(&self) -> &RedissonExpirableInner;

    fn expire(&self, seconds: u64) {
        println!("expire key={} seconds={}", self.get_name(), seconds);
    }
}

// 实现上层 required method 的 blanket impl：
// 任何实现了 RExpirableLike 的类型，自动满足 RObjectLike
impl<T: RExpirableLike> RObjectLike for T {
    fn object_inner(&self) -> &RedissonObjectInner {
        &self.expirable_inner().base
    }
}

// ============================================================
// 第三层：对应 Java RedissonBaseLock
// ============================================================

struct LockRenewalScheduler;

struct RedissonBaseLockInner {
    base: RedissonExpirableInner,
    id: String,
    entry_name: String,
    renewal_scheduler: LockRenewalScheduler,
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

    // 抽象方法：子类必须各自实现
    fn try_lock_inner_async(&self, thread_id: u64) -> bool;
}

// blanket impl：实现了 RedissonBaseLockLike 的类型自动满足 RExpirableLike
impl<T: RedissonBaseLockLike> RExpirableLike for T {
    fn expirable_inner(&self) -> &RedissonExpirableInner {
        &self.lock_inner().base
    }
}

// ============================================================
// 具体类：对应 Java RedissonLock
// ============================================================

struct RedissonLock {
    inner: RedissonBaseLockInner,
}

impl RedissonLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonLock {
            inner: RedissonBaseLockInner {
                base: RedissonExpirableInner {
                    base: RedissonObjectInner {
                        command_executor: CommandAsyncExecutor,
                        name: name.to_string(),
                        codec: "default".to_string(),
                    },
                },
                id: id.to_string(),
                entry_name: format!("{}:{}", id, name),
                renewal_scheduler: LockRenewalScheduler,
            },
        }
    }
}

// 只需实现一个 required method + 一个抽象方法
// RExpirableLike 和 RObjectLike 全部通过 blanket impl 自动满足
impl RedissonBaseLockLike for RedissonLock {
    fn lock_inner(&self) -> &RedissonBaseLockInner {
        &self.inner
    }

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
// 具体类：对应 Java RedissonFairLock extends RedissonLock
// ============================================================

struct RedissonFairLock {
    inner: RedissonBaseLockInner,
}

impl RedissonFairLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonFairLock {
            inner: RedissonBaseLockInner {
                base: RedissonExpirableInner {
                    base: RedissonObjectInner {
                        command_executor: CommandAsyncExecutor,
                        name: name.to_string(),
                        codec: "default".to_string(),
                    },
                },
                id: id.to_string(),
                entry_name: format!("{}:{}", id, name),
                renewal_scheduler: LockRenewalScheduler,
            },
        }
    }
}

impl RedissonBaseLockLike for RedissonFairLock {
    fn lock_inner(&self) -> &RedissonBaseLockInner {
        &self.inner
    }

    fn try_lock_inner_async(&self, thread_id: u64) -> bool {
        println!(
            "RedissonFairLock: fair try lock key={} lock_name={}",
            self.get_name(),
            self.get_lock_name(thread_id)
        );
        true
    }
}

// ============================================================
// 验证
// ============================================================

fn main() {
    let lock = RedissonLock::new("my-lock", "client-1");

    // 来自 RObjectLike（两层 blanket impl 穿透）
    println!("name: {}", lock.get_name());
    // 来自 RExpirableLike（一层 blanket impl 穿透）
    lock.expire(30);

    // 来自 RedissonBaseLockLike default method
    println!("entry_name: {}", lock.get_entry_name());
    println!("lock_name: {}", lock.get_lock_name(42));
    lock.schedule_expiration_renewal(42);

    // RedissonLock 自己实现的抽象方法
    lock.try_lock_inner_async(42);

    println!("---");

    let fair_lock = RedissonFairLock::new("my-fair-lock", "client-1");

    // 所有上层方法同样白得
    fair_lock.expire(30);
    fair_lock.schedule_expiration_renewal(42);
    fair_lock.try_lock_inner_async(42);
}
