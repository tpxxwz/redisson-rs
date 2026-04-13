/// 在 trait_inheritance.rs 基础上，用 Deref 链补全字段穿透访问
///
/// trait + blanket impl 解决：接口继承（方法自动向上传递）
/// Deref 链解决：字段继承（直接访问祖先层字段，无需 .base.base.field）
///
/// Java 继承链：
///   RedissonObject (abstract)
///     └── RedissonExpirable (abstract)
///           └── RedissonBaseLock (abstract)
///                 ├── RedissonLock (concrete)
///                 └── RedissonFairLock (concrete)

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

impl RedissonObjectInner {
    // struct 上定义了 describe()
    fn describe(&self) -> &str {
        "from RedissonObjectInner struct method"
    }
}

trait RObjectLike {
    fn object_inner(&self) -> &RedissonObjectInner;

    fn get_name(&self) -> &str {
        &self.object_inner().name
    }

    // trait 上也定义了同名 describe()，有 default 实现
    fn describe(&self) -> &str {
        "from RObjectLike trait default method"
    }
}

// ============================================================
// 第二层：对应 Java RedissonExpirable
// ============================================================

struct RedissonExpirableInner {
    base: RedissonObjectInner,
}

// Deref：RedissonExpirableInner -> RedissonObjectInner
// 访问 expirable_inner.name 自动穿透到 expirable_inner.base.name
impl Deref for RedissonExpirableInner {
    type Target = RedissonObjectInner;
    fn deref(&self) -> &RedissonObjectInner {
        &self.base
    }
}

trait RExpirableLike: RObjectLike {
    fn expirable_inner(&self) -> &RedissonExpirableInner;

    fn expire(&self, seconds: u64) {
        // 直接用 self.expirable_inner().name，Deref 自动穿透到 ObjectInner.name
        println!("expire key={} seconds={}", self.expirable_inner().name, seconds);
    }
}

// blanket impl：实现了 RExpirableLike 自动满足 RObjectLike
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

// Deref：RedissonBaseLockInner -> RedissonExpirableInner -> RedissonObjectInner
// 访问 lock_inner.name 自动穿透两层到 ObjectInner.name
impl Deref for RedissonBaseLockInner {
    type Target = RedissonExpirableInner;
    fn deref(&self) -> &RedissonExpirableInner {
        &self.base
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
        // 直接用 lock_inner().name，Deref 链自动穿透到 ObjectInner.name
        println!(
            "renewal scheduled: key={} lock={} thread={}",
            self.lock_inner().name,  // 穿透两层：BaseLockInner -> ExpirableInner -> ObjectInner
            self.get_entry_name(),
            thread_id
        );
    }

    // 抽象方法：子类必须各自实现
    fn try_lock_inner_async(&self, thread_id: u64) -> bool;
}

// blanket impl：实现了 RedissonBaseLockLike 自动满足 RExpirableLike
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

// Deref：RedissonLock -> RedissonBaseLockInner -> RedissonExpirableInner -> RedissonObjectInner
// 访问 lock.name / lock.id / lock.entry_name 全部自动穿透
impl Deref for RedissonLock {
    type Target = RedissonBaseLockInner;
    fn deref(&self) -> &RedissonBaseLockInner {
        &self.inner
    }
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

impl RedissonBaseLockLike for RedissonLock {
    fn lock_inner(&self) -> &RedissonBaseLockInner {
        &self.inner
    }

    fn try_lock_inner_async(&self, thread_id: u64) -> bool {
        println!(
            "RedissonLock: try lock key={} lock_name={}",
            self.name,                   // Deref 链直接穿透到 ObjectInner.name
            self.get_lock_name(thread_id)
        );
        true
    }
}

// ============================================================
// 具体类：对应 Java RedissonFairLock
// ============================================================

struct RedissonFairLock {
    inner: RedissonBaseLockInner,
}

impl Deref for RedissonFairLock {
    type Target = RedissonBaseLockInner;
    fn deref(&self) -> &RedissonBaseLockInner {
        &self.inner
    }
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
            self.name,                   // 同样直接穿透
            self.get_lock_name(thread_id)
        );
        true
    }
}

// ============================================================
// 坑：同名方法时 trait 优先于 Deref
// ============================================================
//
// 如果 struct 上有方法 foo()，同时 trait 的 default method 也叫 foo()，
// 直接调用 self.foo() 会走 trait 的版本，struct 上的 foo() 被遮蔽。
// 必须用完全限定语法才能显式调用 struct 上的版本。


// ============================================================
// 验证
// ============================================================

fn main() {
    let lock = RedissonLock::new("my-lock", "client-1");

    // 字段直接穿透（Deref 链）
    println!("name via Deref:       {}", lock.name);       // RedissonLock -> BaseLock -> Expirable -> Object
    println!("id via Deref:         {}", lock.id);         // RedissonLock -> BaseLock
    println!("entry_name via Deref: {}", lock.entry_name); // RedissonLock -> BaseLock

    // trait 方法（blanket impl）
    println!("name via trait:  {}", lock.get_name());
    lock.expire(30);
    lock.schedule_expiration_renewal(42);
    lock.try_lock_inner_async(42);

    println!("---");

    let fair_lock = RedissonFairLock::new("my-fair-lock", "client-1");
    println!("name via Deref: {}", fair_lock.name);
    fair_lock.expire(30);
    fair_lock.try_lock_inner_async(42);

    println!("---");

    // 坑：同名方法，trait 优先于 Deref
    //
    // lock.describe() 看起来像是会通过 Deref 链穿透到 RedissonObjectInner.describe()
    // 实际上走的是 RObjectLike 的 trait default method，struct 上的方法被完全遮蔽
    println!("lock.describe() = {}", lock.describe());
    // 输出：from RObjectLike trait default method
    //       ← 走的是 trait，不是 struct

    // 如果确实需要调用 struct 上的方法，必须用完全限定语法显式指定
    println!(
        "RedissonObjectInner::describe() = {}",
        RedissonObjectInner::describe(lock.object_inner())
    );
    // 输出：from RedissonObjectInner struct method
    //       ← 通过完全限定语法绕过 trait 遮蔽
}
