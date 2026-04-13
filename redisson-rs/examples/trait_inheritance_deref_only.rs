/// 方案：每层直接是 struct，方法写在 impl 块里，通过 Deref 向上"继承"方法。
///
/// 对比 trait_inheritance_no_blanket.rs：
///   - 不再有 XxxInner / XxxLike，结构更扁平
///   - 调用父层方法靠 Deref 自动转发，无需手写 accessor
///
/// 核心问题（见文末 main）：
///   1. 无法实现 trait（用户已知，不展开）
///   2. Deref 不是虚派发 —— 父层方法调 self.xxx()，永远绑定到父层，
///      子类"覆写"对父层完全不可见

use std::ops::Deref;

// ============================================================
// 第一层：RedissonObject
// ============================================================

struct CommandAsyncExecutor;

struct RedissonObject {
    command_executor: CommandAsyncExecutor,
    name: String,
    codec: String,
}

impl RedissonObject {
    fn get_name(&self) -> &str {
        &self.name
    }

    /// 父层方法，内部调用 self.get_name()
    fn describe(&self) {
        // self 这里类型永远是 &RedissonObject
        // 即使从 RedissonFairLock 通过 Deref 链调进来也一样
        println!("[RedissonObject::describe] name={}", self.get_name());
    }
}

// ============================================================
// 第二层：RedissonExpirable
// ============================================================

struct RedissonExpirable {
    inner: RedissonObject,
}

impl Deref for RedissonExpirable {
    type Target = RedissonObject;
    fn deref(&self) -> &RedissonObject {
        &self.inner
    }
}

impl RedissonExpirable {
    fn expire(&self, seconds: u64) {
        println!("expire key={} seconds={}", self.get_name(), seconds);
    }
}

// ============================================================
// 第三层：RedissonBaseLock
// ============================================================

struct LockRenewalScheduler;

struct RedissonBaseLock {
    inner: RedissonExpirable,
    id: String,
    entry_name: String,
    renewal_scheduler: LockRenewalScheduler,
}

impl Deref for RedissonBaseLock {
    type Target = RedissonExpirable;
    fn deref(&self) -> &RedissonExpirable {
        &self.inner
    }
}

impl RedissonBaseLock {
    fn get_entry_name(&self) -> &str {
        &self.entry_name
    }

    fn get_lock_name(&self, thread_id: u64) -> String {
        format!("{}:{}", self.id, thread_id)
    }

    fn schedule_expiration_renewal(&self, thread_id: u64) {
        println!(
            "renewal: lock={} thread={}",
            self.get_entry_name(),
            thread_id
        );
    }

    /// 问题演示：这里内部调用 self.expire()
    /// self 类型是 &RedissonBaseLock，Deref 到 &RedissonExpirable 后调用
    /// RedissonExpirable::expire，永远不会调到子类覆写的版本
    fn lock_and_expire(&self, seconds: u64) {
        println!("lock_and_expire: about to call expire");
        self.expire(seconds); // 永远是 RedissonExpirable::expire
    }
}

// ============================================================
// 第四层：RedissonLock
// ============================================================

struct LockPubSub;

struct RedissonLock {
    inner: RedissonBaseLock,
    internal_lock_lease_time: u64,
    pub_sub: LockPubSub,
}

impl Deref for RedissonLock {
    type Target = RedissonBaseLock;
    fn deref(&self) -> &RedissonBaseLock {
        &self.inner
    }
}

impl RedissonLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonLock {
            inner: RedissonBaseLock {
                inner: RedissonExpirable {
                    inner: RedissonObject {
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
        }
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
// 第五层：RedissonFairLock
// ============================================================

struct RedissonFairLock {
    inner: RedissonLock,
    thread_wait_time: u64,
    threads_queue_name: String,
    timeout_set_name: String,
}

impl Deref for RedissonFairLock {
    type Target = RedissonLock;
    fn deref(&self) -> &RedissonLock {
        &self.inner
    }
}

impl RedissonFairLock {
    fn new(name: &str, id: &str) -> Self {
        RedissonFairLock {
            inner: RedissonLock::new(name, id),
            thread_wait_time: 5_000,
            threads_queue_name: format!("redisson_lock_queue:{}", name),
            timeout_set_name: format!("redisson_lock_timeout:{}", name),
        }
    }

    fn get_threads_queue_name(&self) -> &str {
        &self.threads_queue_name
    }

    /// RedissonFairLock "覆写" expire
    /// 直接在 fair_lock 上调用 .expire() 确实会走这里（方法遮蔽）
    fn expire(&self, seconds: u64) {
        println!(
            "RedissonFairLock: expire key={} seconds={} (fair override)",
            self.get_name(),
            seconds
        );
    }

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
// main：验证 + 问题演示
// ============================================================

fn main() {
    let lock = RedissonLock::new("my-lock", "client-1");
    println!("=== RedissonLock ===");
    println!("name: {}", lock.get_name());            // Deref 到 RedissonObject
    println!("entry: {}", lock.get_entry_name());     // Deref 到 RedissonBaseLock
    lock.expire(30);                                  // Deref 到 RedissonExpirable
    lock.describe();                                  // Deref 到 RedissonObject
    lock.try_lock_inner_async(42);

    println!();
    println!("=== RedissonFairLock ===");
    let fair = RedissonFairLock::new("my-fair-lock", "client-1");
    println!("name: {}", fair.get_name());
    println!("queue: {}", fair.get_threads_queue_name());

    // 直接调用：走 RedissonFairLock::expire（方法遮蔽生效）
    println!("-- 直接调用 fair.expire() --");
    fair.expire(30);

    // 问题：lock_and_expire 定义在 RedissonBaseLock，内部调 self.expire()
    // self 类型已固定为 &RedissonBaseLock，FairLock 的覆写完全不可见
    println!("-- 通过 BaseLock 方法间接调用 --");
    fair.lock_and_expire(30); // 输出的是 RedissonExpirable::expire，不是 FairLock 的版本

    // 问题：无法多态。下面无法编译——RedissonLock 和 RedissonFairLock 是两个不同类型
    // let locks: Vec<&???> = vec![&lock, &fair]; // 没有公共 trait，无法放进同一集合
}
