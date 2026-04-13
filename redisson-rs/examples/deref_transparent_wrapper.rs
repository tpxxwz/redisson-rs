/// 演示 Deref 让包装器对外透明
///
/// 对标 fred 的 WithOptions<C>：
/// 用户拿到包装器之后，直接当原始类型用，感知不到包装层的存在。

use std::ops::Deref;

// ============================================================
// 原始 Client，提供基础命令
// ============================================================

struct Client {
    id: String,
}

impl Client {
    fn new(id: &str) -> Self {
        Client { id: id.to_string() }
    }

    fn get(&self, key: &str) -> String {
        format!("[{}] GET {}", self.id, key)
    }

    fn set(&self, key: &str, value: &str) {
        println!("[{}] SET {} = {}", self.id, key, value);
    }
}

// ============================================================
// 包装器：WithOptions，携带额外配置
// ============================================================

struct Options {
    timeout_ms: u64,
    max_attempts: u32,
}

struct WithOptions<C> {
    client: C,
    options: Options,
}

impl<C> WithOptions<C> {
    fn options(&self) -> &Options {
        &self.options
    }
}

// Deref 让 WithOptions<C> 透明暴露内层 C 的所有方法
// 用户调 with_options.get() 时，找不到就自动穿透到 client.get()
impl<C> Deref for WithOptions<C> {
    type Target = C;
    fn deref(&self) -> &C {
        &self.client
    }
}

// ============================================================
// 给 Client 加一个 with_options() 入口，对标 fred 的用法
// ============================================================

impl Client {
    fn with_options(&self, options: Options) -> WithOptions<&Client> {
        WithOptions {
            client: self,
            options,
        }
    }
}

// ============================================================
// 验证
// ============================================================

fn main() {
    let client = Client::new("client-1");

    let options = Options {
        timeout_ms: 500,
        max_attempts: 3,
    };

    let with_options = client.with_options(options);

    // 用户完全感知不到 WithOptions 这层包装
    // get() / set() 都是通过 Deref 穿透到内层 Client 上的
    println!("{}", with_options.get("foo"));
    with_options.set("bar", "hello");

    // 需要用到 options 时才显式访问
    println!(
        "timeout={}ms max_attempts={}",
        with_options.options().timeout_ms,
        with_options.options().max_attempts,
    );
}
