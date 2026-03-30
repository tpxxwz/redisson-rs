// 演示：为什么 impl Future 返回值会导致 trait 不是对象安全的
// 以及为什么要用泛型 Arc<CE> 而不是 Arc<dyn MyExecutor>
//
// ── 核心结论速查 ────────────────────────────────────────────────
//
// 【何时用泛型 Arc<CE: Trait>，何时用 Arc<dyn Trait>】
//
//   用泛型的条件（满足任一即应考虑泛型）：
//   - trait 含 async fn / impl Future 返回值 → 非对象安全，Arc<dyn> 编译不过
//   - 同一 runtime 里同时存在多种实现（如 CommandAsyncExecutor：
//     CommandAsyncService / CommandBatchService / CommandTransactionService
//     被不同对象同时持有，不是"选一种"而是"并存多种"）
//
//   用 Arc<dyn> 的条件（满足以下特征适合 dyn）：
//   - 运行时根据配置选一种实现，全局唯一（如 ConnectionManager：
//     Single / Cluster / Sentinel 启动时选定，之后不变）
//   - trait 方法无 impl Future（或加 #[async_trait] 自动转成 Pin<Box<dyn Future>>）
//   - 不希望泛型参数向上传染整个调用链
//
//   注意：#[async_trait] 宏把 async fn 改写成 -> Pin<Box<dyn Future>>，
//         使 trait 变成对象安全，代价是每次调用一次 Box 堆分配。
//
// 【Pin 核心结论】
//
//   async fn 写法 → 不用管 Pin，编译器在 .await 时自动 pin 住 Future 再 poll
//
//   需要手写 Pin 的两种情况：
//   1. 手动实现 Future trait（需要在 poll 方法里处理 Pin<&mut Self>）
//   2. 把 Future 存起来当 dyn 用（需要 Pin<Box<dyn Future>>）
//      原因：async 生成的状态机可能含自引用字段（如 let r = &s; 跨 await 点）
//            一旦 .await 开始，状态机在内存中的地址不能移动，Pin 在类型系统层面
//            保证这块内存不会被 move
//
// ────────────────────────────────────────────────────────────────

use std::future::Future;
use std::sync::Arc;

// ============================================================
// 1. 定义一个返回 impl Future 的 trait（非对象安全）
// ============================================================

trait MyExecutor {
    // 返回 impl Future —— 编译器会为每个实现类生成不同的具体 Future 类型
    // 这导致 trait 不是"对象安全"的（object-safe）
    fn execute(&self, key: &str) -> impl Future<Output = String> + Send;
}

// ============================================================
// 2. 两个具体实现
// ============================================================

struct RealExecutor;

impl MyExecutor for RealExecutor {
    async fn execute(&self, key: &str) -> String {
        format!("RealExecutor: got key={}", key)
    }
}

struct MockExecutor;

impl MyExecutor for MockExecutor {
    async fn execute(&self, key: &str) -> String {
        format!("MockExecutor: got key={}", key)
    }
}

// ============================================================
// 3. 正确做法：泛型参数 <CE: MyExecutor>
//    Arc<CE> clone 只是引用计数 +1，底层同一个实例
//
// 对应 redisson-rs 中的 CommandAsyncExecutor 设计选择：
//
//   选泛型 Arc<CE: CommandAsyncExecutor>，不选 Arc<dyn CommandAsyncExecutorDyn>
//
//   两种方案对比：
//
//   泛型方案 Arc<CE: Trait>：
//   ┌────────────────┬──────────────────────────────────────────────────────────────────────────────────────────────────┐
//   │      维度      │                                               表现                                               │
//   ├────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ 性能           │ 零额外开销，编译期单态化                                                                         │
//   ├────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ 灵活性         │ 编译期固定类型，一个 RedissonObject<RealExecutor> 和 RedissonObject<MockExecutor> 是两个不同类型 │
//   ├────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ 存进同一个 Vec │ 不行，类型不同无法混放                                                                           │
//   ├────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ 类型签名传染   │ <CE> 会一路向上传染，持有它的结构体都要加泛型参数                                                │
//   ├────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────┤
//   │ 适用场景       │ 运行时只有一种实现（生产用 RealExecutor，测试用 MockExecutor，不会同时混用）                     │
//   └────────────────┴──────────────────────────────────────────────────────────────────────────────────────────────────┘
//
//   dyn 方案 Arc<dyn TraitDyn>：
//   ┌────────────────┬──────────────────────────────────────────────────────────────────┐
//   │      维度      │                                 表现                             │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 性能           │ 每次调用多一次 vtable 查找 + Box 堆分配                          │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 灵活性         │ 运行时可换实现，同一个 Vec 可以混放不同实现                      │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 存进同一个 Vec │ 可以，Vec<Arc<dyn MyExecutorDyn>>                                │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 类型签名传染   │ 不传染，持有方不需要泛型参数                                     │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 代价           │ trait 方法必须改签名，返回值从 impl Future 改成 Pin<Box<dyn Future>> │
//   ├────────────────┼──────────────────────────────────────────────────────────────────┤
//   │ 适用场景       │ 运行时需要多种实现混用，或者结构体层级很深、不想让泛型参数一路传染 │
//   └────────────────┴──────────────────────────────────────────────────────────────────┘
//
//   选泛型的具体原因：
//
//   原因一【不得不用泛型】：
//     CommandAsyncExecutor 的方法返回 impl Future（async fn），
//     导致该 trait 不是对象安全的，Arc<dyn CommandAsyncExecutor> 根本编译不过。
//     若要用 dyn，必须把所有方法签名改成返回 Pin<Box<dyn Future>>，
//     这会侵入并修改整个 trait 定义，代价较高。
//
//   原因二【运行时不需要混用实现】：
//     整个 Redisson runtime 里 CommandAsyncExecutor 只有一种实现
//     （FredCommandExecutor），不存在同一个 Vec 里混放多种实现的需求。
//     dyn 的核心优势（运行时多态、混放）在这里用不到。
//
//   原因三【泛型传染可接受】：
//     RedissonObject<CE>、RedissonLock<CE> 等都带泛型参数，
//     但这些类型的使用方（Redisson 入口）在编译期已知 CE 的具体类型，
//     传染链条有限，不会带来实际麻烦。
//
//   原因四【零额外运行时开销】：
//     泛型在编译期单态化，不产生 vtable 查找和 Box 堆分配，
//     而 dyn 方案每次调用都需要一次 Box 堆分配（几十 ns）。
//     虽然相比 Redis RTT（几百 µs）可忽略不计，但泛型方案在代码清晰度上更优。
// ============================================================

struct RedissonObject<CE: MyExecutor> {
    executor: Arc<CE>,
    name: String,
}

impl<CE: MyExecutor> RedissonObject<CE> {
    fn new(executor: &Arc<CE>, name: &str) -> Self {
        Self {
            executor: executor.clone(), // Arc clone：引用计数 +1，不拷贝数据
            name: name.to_string(),
        }
    }

    async fn do_something(&self) -> String {
        self.executor.execute(&self.name).await
    }
}

// ============================================================
// 4. 错误做法（注释掉，打开会编译报错）：
//    试图用 dyn MyExecutor
// ============================================================

// fn broken(executor: Arc<dyn MyExecutor>) {
//     // 编译错误：
//     // the trait `MyExecutor` cannot be made into an object
//     // because method `execute` has a `impl Trait` return type
// }

// ============================================================
// 5. 如果确实需要 dyn（对象安全），必须改成 Box<dyn Future>
// ============================================================

trait MyExecutorDyn {
    // 用 Box<dyn Future> 替代 impl Future，牺牲一次堆分配换来对象安全
    fn execute_dyn(&self, key: &str) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>>;
}

impl MyExecutorDyn for RealExecutor {
    fn execute_dyn(&self, key: &str) -> std::pin::Pin<Box<dyn Future<Output = String> + Send + '_>> {
        let key = key.to_string();
        Box::pin(async move { format!("RealExecutor(dyn): got key={}", key) })
    }
}

struct RedissonObjectDyn {
    executor: Arc<dyn MyExecutorDyn>, // 现在可以用 dyn 了
    name: String,
}

impl RedissonObjectDyn {
    fn new(executor: Arc<dyn MyExecutorDyn>, name: &str) -> Self {
        Self { executor, name: name.to_string() }
    }

    async fn do_something(&self) -> String {
        self.executor.execute_dyn(&self.name).await
    }
}

// ============================================================
// 6. 演示：async fn 自动处理 Pin vs 手动 Pin<Box<dyn Future>>
// ============================================================

// 情况A：普通 async fn，编译器自动处理 Pin，调用方直接 .await 即可
async fn auto_pinned(x: u32) -> u32 {
    let s = format!("value={}", x);   // s 在状态机里
    let _r = &s;                       // 自引用：跨 await 点引用 s
    tokio::task::yield_now().await;    // await 点：状态机可能被挂起，但 Pin 由编译器保证
    x * 2
}

// 情况B：需要把 Future 存进结构体或作为 dyn 传递时，必须手写 Pin<Box<dyn Future>>
struct StoredFuture {
    // 不能写 future: dyn Future<...>      → 大小不固定，编译报错
    // 不能写 future: Box<dyn Future<...>> → 没有 Pin 保证，poll 不安全
    future: std::pin::Pin<Box<dyn Future<Output = u32> + Send>>,
}

impl StoredFuture {
    fn new(x: u32) -> Self {
        // Box::pin 同时完成：分配到堆 + Pin 住，一步到位
        Self { future: Box::pin(auto_pinned(x)) }
    }

    async fn run(self) -> u32 {
        self.future.await
    }
}

// ============================================================
// main
// ============================================================

#[tokio::main]
async fn main() {
    // --- 泛型方案（项目实际采用）---
    let executor = Arc::new(RealExecutor);
    let obj1 = RedissonObject::new(&executor, "my-lock");
    let obj2 = RedissonObject::new(&executor, "my-bucket");
    // executor、obj1.executor、obj2.executor 三个 Arc 指向同一个 RealExecutor
    // Arc 强引用计数此时为 3，RealExecutor 只有一份

    println!("=== 泛型方案 ===");
    println!("{}", obj1.do_something().await);
    println!("{}", obj2.do_something().await);

    // --- dyn 方案（需要改 trait 签名，有 Box 堆分配开销）---
    let executor_dyn: Arc<dyn MyExecutorDyn> = Arc::new(RealExecutor);
    let obj3 = RedissonObjectDyn::new(executor_dyn, "my-map");

    println!("\n=== dyn 方案（Box<dyn Future>）===");
    println!("{}", obj3.do_something().await);

    // --- 泛型方案支持换实现（编译期确定，零运行时开销）---
    let mock_executor = Arc::new(MockExecutor);
    let mock_obj = RedissonObject::new(&mock_executor, "test-key");

    println!("\n=== 换成 MockExecutor ===");
    println!("{}", mock_obj.do_something().await);

    // --- Pin 演示 ---
    println!("\n=== async fn 自动 Pin（无需手写）===");
    // 直接 .await，编译器在内部自动 pin 住状态机
    let result = auto_pinned(21).await;
    println!("auto_pinned(21) = {}", result);

    println!("\n=== 手动 Pin<Box<dyn Future>>（Future 存进结构体）===");
    // Future 需要存进结构体时，必须用 Box::pin
    let stored = StoredFuture::new(21);
    let result = stored.run().await;
    println!("StoredFuture(21).run() = {}", result);
}
