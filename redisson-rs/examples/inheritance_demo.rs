//! Rust 模拟 Java 多层继承的完整示例
//!
//! 涵盖内容：
//! 1. 接口 (trait Lock)
//! 2. 字段继承 (组合 + Deref)
//! 3. trait 默认实现 (模板方法)
//! 4. 多层抽象类 (trait 继承链)
//!
//! Java 结构：
//! ```java
//! // 1. 接口
//! interface Lock {
//!     void lock();
//!     void unlock();
//!     String getName();
//! }
//!
//! // 2. 抽象基类
//! abstract class AbstractLock implements Lock {
//!     protected String name;
//!     protected long timeout;
//!
//!     // 模板方法
//!     public void lock() {
//!         if (tryAcquire()) onAcquired();
//!         else waitAndRetry();
//!     }
//!
//!     // 抽象方法
//!     abstract boolean tryAcquire();
//!
//!     // 钩子方法
//!     void onAcquired() { System.out.println("default"); }
//!     void waitAndRetry() { System.out.println("waiting"); }
//! }
//!
//! // 3. 子类
//! class SimpleLock extends AbstractLock {
//!     protected String ownerId;  // 新增字段
//!     boolean tryAcquire() { /* 实现 */ }
//!     void onAcquired() { /* 覆盖 */ }
//! }
//!
//! // 4. 子类的子类
//! class ReentrantLock extends SimpleLock {
//!     private int holdCount;  // 新增字段
//!     boolean tryAcquire() { /* 覆盖 */ }
//!     void onAcquired() { /* 覆盖 */ }
//!     void unlock() { /* 覆盖 */ }
//! }
//! ```

use std::ops::Deref;
use std::sync::Mutex;

// ============================================================
// 第一部分：接口 (trait Lock)
// ============================================================

/// 对应 Java interface Lock
pub trait Lock: Send + Sync {
    fn name(&self) -> &str;
    fn lock(&self);
    fn unlock(&self);
}

// ============================================================
// 第二部分：字段继承 (组合 + Deref)
// ============================================================

/// 对应 Java AbstractLock 的 protected 字段
pub struct AbstractLockFields {
    pub name: String,
    pub timeout_ms: u64,
    locked: Mutex<bool>,
}

impl AbstractLockFields {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            timeout_ms: 30000,
            locked: Mutex::new(false),
        }
    }

    /// 自己的方法：调用 Lock trait 的方法
    pub fn lock_and_log(&self)
    where
        Self: Lock,
    {
        println!("[AbstractLockFields] lock_and_log: {}", self.name);
        self.lock();  // 调用 Lock trait 的 lock()
    }

    /// 自己的方法：调用 AbstractLock trait 的方法
    pub fn try_lock_with_timeout(&self, timeout_ms: u64) -> bool
    where
        Self: AbstractLock,
    {
        println!("[AbstractLockFields] tryLock with timeout: {}ms", timeout_ms);
        self.try_acquire()  // 调用 AbstractLock trait 的方法
    }
}

// AbstractLockFields 自己也实现 Lock 和 AbstractLock
// 这样它的方法可以调用 trait 方法
impl Lock for AbstractLockFields {
    fn name(&self) -> &str {
        &self.name
    }

    fn lock(&self) {
        self.do_lock();
    }

    fn unlock(&self) {
        let mut locked = self.locked.lock().unwrap();
        *locked = false;
        println!("[AbstractLockFields] unlock: {}", self.name);
    }
}

impl AbstractLock for AbstractLockFields {
    fn try_acquire(&self) -> bool {
        let mut locked = self.locked.lock().unwrap();
        if *locked {
            return false;
        }
        *locked = true;
        println!("[AbstractLockFields] tryAcquire: {}", self.name);
        true
    }

    // on_acquired, wait_and_retry 使用默认实现
}

// ============================================================
// 第三部分：trait 默认实现 (模板方法)
// ============================================================

/// 对应 Java abstract class AbstractLock implements Lock
/// trait 继承 Lock，添加抽象方法和钩子方法
pub trait AbstractLock: Lock {
    // ── 抽象方法：子类必须实现 ──
    fn try_acquire(&self) -> bool;

    // ── 钩子方法：子类可选覆盖 ──
    fn on_acquired(&self) {
        println!("[AbstractLock] default onAcquired: {}", self.name());
    }

    fn wait_and_retry(&self) {
        println!("[AbstractLock] default waitAndRetry: {}", self.name());
    }

    // ── 模板方法（在 impl Lock 中调用）──
    fn do_lock(&self) {
        println!("[AbstractLock] lock() for '{}'", self.name());
        if self.try_acquire() {
            self.on_acquired();
        } else {
            self.wait_and_retry();
        }
    }
}

// ============================================================
// 第四部分：多层抽象类 (trait 继承链)
// ============================================================

/// 对应 Java abstract class AbstractReentrantLock extends AbstractLock
pub trait AbstractReentrantLock: AbstractLock {
    // ── 抽象方法：孙类必须实现 ──
    fn fair_wait(&self);

    // ── 访问器 ──
    fn hold_count(&self) -> i32;

    // ── 可选覆盖 ──
    fn try_acquire_reentrant(&self) -> bool {
        println!(
            "[AbstractReentrantLock] default tryAcquire, holdCount={}",
            self.hold_count()
        );
        true
    }

    fn on_acquired_reentrant(&self) {
        println!(
            "[AbstractReentrantLock] default onAcquired, holdCount={}",
            self.hold_count()
        );
    }
}

// ============================================================
// 第五部分：SimpleLock (子类)
// ============================================================

/// 对应 Java class SimpleLock extends AbstractLock
pub struct SimpleLock {
    pub base: AbstractLockFields,  // 组合基类字段
    owner_id: Mutex<String>,        // 自己的字段
}

// ⭐ Deref：让 SimpleLock 可以直接访问 AbstractLockFields 的字段
impl Deref for SimpleLock {
    type Target = AbstractLockFields;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl SimpleLock {
    pub fn new(name: &str) -> Self {
        Self {
            base: AbstractLockFields::new(name),
            owner_id: Mutex::new(String::new()),
        }
    }

    pub fn get_owner_id(&self) -> String {
        self.owner_id.lock().unwrap().clone()
    }
}

// impl Lock：必须实现
impl Lock for SimpleLock {
    fn name(&self) -> &str {
        &self.base.name  // 或者 self.name (通过 Deref)
    }

    fn lock(&self) {
        self.do_lock();  // 调用 AbstractLock 的模板方法
    }

    fn unlock(&self) {
        println!("[SimpleLock] unlock: {}", self.name);
        *self.owner_id.lock().unwrap() = String::new();
    }
}

// impl AbstractLock：必须实现抽象方法，可选覆盖钩子
impl AbstractLock for SimpleLock {
    fn try_acquire(&self) -> bool {
        println!("[SimpleLock] tryAcquire: {}", self.name);
        true
    }

    fn on_acquired(&self) {
        *self.owner_id.lock().unwrap() = format!("owner-{}", self.name);
        println!("[SimpleLock] onAcquired: ownerId set to '{}'", self.get_owner_id());
    }

    // wait_and_retry 使用默认实现
}

// ============================================================
// 第六部分：ReentrantLock (孙类)
// ============================================================

/// 对应 Java class ReentrantLock extends SimpleLock
pub struct ReentrantLock {
    pub base: SimpleLock,       // 组合父类
    hold_count: Mutex<i32>,     // 自己的字段
}

// ⭐ Deref：让 ReentrantLock 可以直接访问 SimpleLock 的字段和方法
// 同时可以链式访问到 AbstractLockFields
impl Deref for ReentrantLock {
    type Target = SimpleLock;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl ReentrantLock {
    pub fn new(name: &str) -> Self {
        Self {
            base: SimpleLock::new(name),
            hold_count: Mutex::new(0),
        }
    }
}

// impl Lock
impl Lock for ReentrantLock {
    fn name(&self) -> &str {
        &self.base.name  // 通过 base.base.name 或 self.name (Deref 链)
    }

    fn lock(&self) {
        self.do_lock();  // 使用 AbstractLock 的模板方法
    }

    fn unlock(&self) {
        let mut count = self.hold_count.lock().unwrap();
        *count = count.saturating_sub(1);
        println!("[ReentrantLock] unlock: holdCount={}", *count);
    }
}

// impl AbstractLock
impl AbstractLock for ReentrantLock {
    fn try_acquire(&self) -> bool {
        <Self as AbstractReentrantLock>::try_acquire_reentrant(self)
    }

    fn on_acquired(&self) {
        <Self as AbstractReentrantLock>::on_acquired_reentrant(self)
    }
}

// impl AbstractReentrantLock
impl AbstractReentrantLock for ReentrantLock {
    fn fair_wait(&self) {
        println!("[ReentrantLock] fairWait: {}", self.name);
    }

    fn hold_count(&self) -> i32 {
        *self.hold_count.lock().unwrap()
    }

    fn try_acquire_reentrant(&self) -> bool {
        let mut count = self.hold_count.lock().unwrap();
        *count += 1;
        println!("[ReentrantLock] tryAcquire: holdCount={}", *count);
        true
    }

    fn on_acquired_reentrant(&self) {
        println!(
            "[ReentrantLock] onAcquired: holdCount={}",
            self.hold_count()
        );
    }
}

// ============================================================
// 第七部分：FairReentrantLock (曾孙类 - 可选)
// ============================================================

/// 只覆盖 fair_wait 的公平锁
pub struct FairReentrantLock {
    pub base: ReentrantLock,
}

impl Deref for FairReentrantLock {
    type Target = ReentrantLock;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl FairReentrantLock {
    pub fn new(name: &str) -> Self {
        Self {
            base: ReentrantLock::new(name),
        }
    }
}

impl Lock for FairReentrantLock {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn lock(&self) {
        self.do_lock();
    }

    fn unlock(&self) {
        <ReentrantLock as Lock>::unlock(&self.base);
    }
}

impl AbstractLock for FairReentrantLock {
    fn try_acquire(&self) -> bool {
        <ReentrantLock as AbstractLock>::try_acquire(&self.base)
    }

    fn on_acquired(&self) {
        <ReentrantLock as AbstractLock>::on_acquired(&self.base)
    }
}

impl AbstractReentrantLock for FairReentrantLock {
    fn fair_wait(&self) {
        println!("[FairReentrantLock] FAIR queue wait for: {}", self.name);
    }

    fn hold_count(&self) -> i32 {
        self.base.hold_count()
    }
}

// ============================================================
// 第八部分：演示
// ============================================================

fn use_as_lock(lock: &(dyn Lock + '_)) {
    println!("  --- Using as &dyn Lock ---");
    lock.lock();
    lock.unlock();
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Rust 模拟 Java 多层继承完整示例                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // ════════════════════════════════════════════════════════════════
    // 1. 字段继承 + Deref
    // ════════════════════════════════════════════════════════════════
    println!("【1. 字段继承 (组合 + Deref)】");
    println!();
    println!("   AbstractLockFields {{ name, timeout_ms }}");
    println!("       ↓ Deref");
    println!("   SimpleLock {{ base: AbstractLockFields, owner_id }}");
    println!("       ↓ Deref");
    println!("   ReentrantLock {{ base: SimpleLock, hold_count }}");
    println!("       ↓ Deref");
    println!("   FairReentrantLock {{ base: ReentrantLock }}");
    println!();

    let reentrant = ReentrantLock::new("test-lock");
    println!("   reentrant.name = '{}'        (通过 Deref 链访问)", reentrant.name);
    println!("   reentrant.timeout_ms = {}    (通过 Deref 链访问)", reentrant.timeout_ms);
    println!("   reentrant.get_owner_id()     (SimpleLock 的方法)");
    println!("   reentrant.hold_count         (自己的字段)");
    println!();

    // ════════════════════════════════════════════════════════════════
    // 1.5 AbstractLockFields 的方法调用 trait 方法
    // ════════════════════════════════════════════════════════════════
    println!("【1.5 抽象类自己的方法调用 trait 方法】");
    println!();
    println!("   impl AbstractLockFields {{");
    println!("       fn lock_and_log(&self) where Self: Lock {{");
    println!("           self.lock();  // 调用 Lock trait 的方法");
    println!("       }}");
    println!("       fn try_lock_with_timeout(&self) where Self: AbstractLock {{");
    println!("           self.try_acquire();  // 调用 AbstractLock trait 的方法");
    println!("       }}");
    println!("   }}");
    println!();

    let base = AbstractLockFields::new("base-lock");
    println!("   base.lock_and_log():");
    base.lock_and_log();  // 调用自己的方法，内部调用 Lock::lock()
    base.unlock();
    println!();

    println!("   base.try_lock_with_timeout(1000):");
    let acquired = base.try_lock_with_timeout(1000);  // 调用 AbstractLock::try_acquire()
    println!("   acquired = {}", acquired);
    base.unlock();
    println!();

    // ════════════════════════════════════════════════════════════════
    // 2. trait 默认实现 (模板方法)
    // ════════════════════════════════════════════════════════════════
    println!("【2. trait 默认实现 (模板方法)】");
    println!();
    println!("   trait AbstractLock: Lock {{");
    println!("       fn try_acquire(&self) -> bool;  // 抽象方法");
    println!("       fn on_acquired(&self) {{ ... }} // 钩子方法");
    println!("       fn wait_and_retry(&self) {{ ... }}");
    println!("       fn do_lock(&self) {{ try_acquire() ? on_acquired() : wait_and_retry(); }}");
    println!("   }}");
    println!();
    println!("   子类只需实现 try_acquire，可选覆盖 on_acquired/wait_and_retry");
    println!();

    // ════════════════════════════════════════════════════════════════
    // 3. 多层抽象类 (trait 继承链)
    // ════════════════════════════════════════════════════════════════
    println!("【3. 多层抽象类 (trait 继承链)】");
    println!();
    println!("   Lock (接口)");
    println!("     ↑");
    println!("   AbstractLock: Lock  (抽象层1)");
    println!("     ↑");
    println!("   AbstractReentrantLock: AbstractLock  (抽象层2)");
    println!("     ↑");
    println!("   ReentrantLock (具体类)");
    println!();

    // ════════════════════════════════════════════════════════════════
    // 4. 实际运行
    // ════════════════════════════════════════════════════════════════
    println!("【4. 实际运行】");
    println!();

    println!("--- SimpleLock (只覆盖 try_acquire, on_acquired) ---");
    let simple = SimpleLock::new("simple-lock");
    simple.lock();
    simple.unlock();
    println!();

    println!("--- ReentrantLock (覆盖更多方法) ---");
    let reentrant = ReentrantLock::new("reentrant-lock");
    reentrant.lock();
    reentrant.lock();  // 重入
    reentrant.unlock();
    println!();

    println!("--- FairReentrantLock (只覆盖 fair_wait) ---");
    let fair = FairReentrantLock::new("fair-lock");
    fair.lock();
    fair.unlock();
    println!();

    // ════════════════════════════════════════════════════════════════
    // 5. 多态
    // ════════════════════════════════════════════════════════════════
    println!("【5. 多态 (作为 &dyn Lock 使用)】");
    println!();

    let locks: Vec<&dyn Lock> = vec![&simple, &reentrant, &fair];
    for lock in locks {
        use_as_lock(lock);
    }

    // ════════════════════════════════════════════════════════════════
    // 6. 总结
    // ════════════════════════════════════════════════════════════════
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  总结                                                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Java                          │  Rust                         ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  1 个接口 Lock                 │  1 个 trait Lock              ║");
    println!("║  1 个抽象类 AbstractLock       │  1 个 trait AbstractLock      ║");
    println!("║  1 个抽象子类 AbstractReentrant│  1 个 trait AbstractReentrant ║");
    println!("║  2 个具体类                    │  3 个 struct                  ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  字段继承                      │  组合 + Deref                 ║");
    println!("║  方法继承                      │  trait 默认实现               ║");
    println!("║  super.xxx()                   │  <Parent as Trait>::xxx()     ║");
    println!("║  多态自动                      │  impl trait (只覆盖需要的)    ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}
