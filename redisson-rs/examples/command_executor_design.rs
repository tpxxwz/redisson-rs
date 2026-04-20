//! # CommandAsyncExecutor 设计方案示例
//!
//! ## Java 的实际结构（Template Method 模式）
//!
//! ```text
//! CommandAsyncExecutor (interface, ~50 methods)
//!     └── CommandAsyncService implements CommandAsyncExecutor
//!             │  所有公共方法（writeAsync / readAsync / evalWriteAsync / ...）
//!             │  最终都委托到内部 async() 方法
//!             │
//!             │  public RFuture writeAsync(String key, ...) {
//!             │      return async(false, nodeSource, codec, command, params, ...);
//!             │  }
//!             │  // ← 所有接口方法都这样
//!             │
//!             │  public RFuture async(boolean readOnly, NodeSource, Codec, ...) {
//!             │      // 真正执行 Redis 命令  ← CommandBatchService 覆写这里
//!             │  }
//!             │
//!             └── CommandBatchService extends CommandAsyncService
//!                     // 只覆写 async()（非接口方法）
//!                     // ~50 个接口方法全部继承，不动
//! ```
//!
//! ## Rust 对应结构
//!
//! ```text
//! CommandDispatch trait（内部钩子，对应 Java protected async() 方法）
//!   fn  inner()    → &CommandAsyncInner
//!   async fn dispatch()                ← CommandBatchService 覆写的那个
//!   fn  is_eval_cache_active()         ← 可选覆写
//!
//! impl<T: CommandDispatch> CommandAsyncExecutor for T {}  ← blanket impl
//!
//! CommandAsyncExecutor trait（对应 Java CommandAsyncExecutor interface）
//!   // 全部方法，全是 default，调用 self.dispatch().await
//!   async fn write_async()      { self.dispatch(false, ...).await }
//!   async fn read_async()       { self.dispatch(true,  ...).await }
//!   async fn eval_write_async() { ... }
//!   fn encode()                 { self.inner().encode(...) }
//!
//! CommandAsyncService: 只 impl CommandDispatch → 真正执行 Redis 命令
//! CommandBatchService: 只 impl CommandDispatch → 缓冲入队
//! 两者都不需要写 impl CommandAsyncExecutor，零重复
//! ```
//!
//! 运行：cargo run --example command_executor_design

use std::sync::{Arc, Mutex};

// ─────────────────────────────────────────────────────────────
// 辅助类型（桩）
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Value(pub String);

#[derive(Clone, Debug)]
pub struct Bytes(pub Vec<u8>);

/// 对应 Java NodeSource
#[derive(Clone, Debug)]
pub struct NodeSource {
    pub slot: u16,
    pub read_only: bool,
}

pub trait ConnectionManager: Send + Sync {
    fn calc_slot(&self, key: &[u8]) -> u16;
    fn use_script_cache(&self) -> bool;
}

pub trait Codec: Send + Sync {
    fn name(&self) -> &str;
}

pub struct StringCodec;
impl Codec for StringCodec {
    fn name(&self) -> &str { "string" }
}

#[derive(Debug)]
pub struct BatchResult {
    pub responses: Vec<Value>,
}

// ─────────────────────────────────────────────────────────────
// CommandAsyncInner — 共享状态 + 纯工具方法
// ─────────────────────────────────────────────────────────────

pub struct CommandAsyncInner {
    pub connection_manager: Arc<dyn ConnectionManager>,
    pub retry_attempts: u32,
    pub track_changes: bool,
}

impl CommandAsyncInner {
    pub fn new(connection_manager: Arc<dyn ConnectionManager>) -> Self {
        Self { connection_manager, retry_attempts: 3, track_changes: false }
    }

    pub fn node_source_for_key(&self, key: &str) -> NodeSource {
        NodeSource {
            slot: self.connection_manager.calc_slot(key.as_bytes()),
            read_only: false,
        }
    }

    pub fn encode(&self, _codec: &dyn Codec, value: Value) -> Bytes {
        Bytes(value.0.into_bytes())
    }
}

// ─────────────────────────────────────────────────────────────
// CommandDispatch — 对应 Java CommandAsyncService.async()
//
// 这是 CommandBatchService 在 Java 中实际覆写的东西：
// 一个非接口方法，是所有公共方法的最终委托点。
//
// pub(crate)：对外完全不可见，不是公开接口的一部分。
// ─────────────────────────────────────────────────────────────

pub(crate) trait CommandDispatch: Send + Sync {
    fn inner(&self) -> &CommandAsyncInner;

    /// 对应 Java CommandAsyncService.async(boolean readOnly, NodeSource, Codec, command, params)
    async fn dispatch(
        &self,
        node_source: NodeSource,
        codec: Arc<dyn Codec>,
        cmd: &'static str,
        args: Vec<Value>,
    ) -> anyhow::Result<Value>;

    fn is_eval_cache_active(&self) -> bool {
        self.inner().connection_manager.use_script_cache()
    }

    fn is_batch(&self) -> bool { false }
}

// ─────────────────────────────────────────────────────────────
// CommandAsyncExecutor — 对应 Java CommandAsyncExecutor interface
//
// 公开接口，不含任何内部方法（dispatch / inner 对调用方不可见）。
// 方法体由下方 blanket impl 统一提供，trait 本身无 default。
// ─────────────────────────────────────────────────────────────

pub trait CommandAsyncExecutor: Send + Sync {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;
    fn is_track_changes(&self) -> bool;   // Java: isTrackChanges() — 在接口上
    // is_eval_cache_active / is_batch 不在接口上（Java 里是 protected / instanceof）
    fn encode(&self, codec: &dyn Codec, value: Value) -> Bytes;
    fn encode_map_key(&self, codec: &dyn Codec, value: Value) -> Bytes;

    async fn write_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value>;
    async fn write_async_no_codec(&self, key: String, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value>;
    async fn read_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value>;
    async fn read_async_no_codec(&self, key: String, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value>;
    async fn eval_write_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, script: String, script_keys: Vec<String>, args: Vec<Value>) -> anyhow::Result<Value>;
    async fn eval_read_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, script: String, script_keys: Vec<String>, args: Vec<Value>) -> anyhow::Result<Value>;
    // ... 剩余方法签名同理
}

// ─────────────────────────────────────────────────────────────
// Blanket impl：凡实现 CommandDispatch，自动获得 CommandAsyncExecutor
//
// dispatch / inner 只在这个 impl 块里用，外部完全看不见。
// ─────────────────────────────────────────────────────────────

impl<T: CommandDispatch> CommandAsyncExecutor for T {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.inner().connection_manager.clone()
    }
    fn is_track_changes(&self) -> bool { self.inner().track_changes }
    fn encode(&self, codec: &dyn Codec, value: Value) -> Bytes { self.inner().encode(codec, value) }
    fn encode_map_key(&self, codec: &dyn Codec, value: Value) -> Bytes { self.inner().encode(codec, value) }

    async fn write_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value> {
        let source = self.inner().node_source_for_key(&key);
        self.dispatch(source, codec, cmd, args).await
    }
    async fn write_async_no_codec(&self, key: String, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value> {
        self.write_async(key, Arc::new(StringCodec), cmd, args).await
    }
    async fn read_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value> {
        let mut source = self.inner().node_source_for_key(&key);
        source.read_only = true;
        self.dispatch(source, codec, cmd, args).await
    }
    async fn read_async_no_codec(&self, key: String, cmd: &'static str, args: Vec<Value>) -> anyhow::Result<Value> {
        self.read_async(key, Arc::new(StringCodec), cmd, args).await
    }
    async fn eval_write_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, script: String, script_keys: Vec<String>, args: Vec<Value>) -> anyhow::Result<Value> {
        let source = self.inner().node_source_for_key(&key);
        let mut all_args = vec![Value(script), Value(script_keys.len().to_string())];
        all_args.extend(script_keys.into_iter().map(Value));
        all_args.extend(args);
        self.dispatch(source, codec, cmd, all_args).await
    }
    async fn eval_read_async(&self, key: String, codec: Arc<dyn Codec>, cmd: &'static str, script: String, script_keys: Vec<String>, args: Vec<Value>) -> anyhow::Result<Value> {
        let mut source = self.inner().node_source_for_key(&key);
        source.read_only = true;
        let mut all_args = vec![Value(script), Value(script_keys.len().to_string())];
        all_args.extend(script_keys.into_iter().map(Value));
        all_args.extend(args);
        self.dispatch(source, codec, cmd, all_args).await
    }
}

// ─────────────────────────────────────────────────────────────
// CommandAsyncService
// ─────────────────────────────────────────────────────────────

pub struct CommandAsyncService {
    pub(crate) inner: CommandAsyncInner,
}

impl CommandAsyncService {
    pub fn new(connection_manager: Arc<dyn ConnectionManager>) -> Self {
        Self { inner: CommandAsyncInner::new(connection_manager) }
    }
}

impl CommandDispatch for CommandAsyncService {
    fn inner(&self) -> &CommandAsyncInner { &self.inner }

    async fn dispatch(
        &self,
        node_source: NodeSource,
        _codec: Arc<dyn Codec>,
        cmd: &'static str,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        let rw = if node_source.read_only { "READ " } else { "WRITE" };
        println!("[Service] {rw}  slot={}  cmd={cmd}  args={args:?}", node_source.slot);
        Ok(Value("OK".to_string()))
    }
}

// ─────────────────────────────────────────────────────────────
// CommandBatchService
// ─────────────────────────────────────────────────────────────

struct BatchEntry {
    node_source: NodeSource,
    cmd: &'static str,
    args: Vec<Value>,
}

pub struct CommandBatchService {
    pub(crate) base: Arc<CommandAsyncService>,
    queue: Mutex<Vec<BatchEntry>>,
}

impl CommandBatchService {
    pub fn new(base: Arc<CommandAsyncService>) -> Self {
        Self { base, queue: Mutex::new(Vec::new()) }
    }

    pub async fn execute_async(&self) -> anyhow::Result<BatchResult> {
        let entries: Vec<BatchEntry> = std::mem::take(&mut *self.queue.lock().unwrap());
        let mut responses = Vec::with_capacity(entries.len());
        for entry in entries {
            let v = self.base.dispatch(entry.node_source, Arc::new(StringCodec), entry.cmd, entry.args).await?;
            responses.push(v);
        }
        Ok(BatchResult { responses })
    }
}

impl CommandDispatch for CommandBatchService {
    fn inner(&self) -> &CommandAsyncInner { &self.base.inner }

    async fn dispatch(
        &self,
        node_source: NodeSource,
        _codec: Arc<dyn Codec>,
        cmd: &'static str,
        args: Vec<Value>,
    ) -> anyhow::Result<Value> {
        println!("[Batch]   QUEUE  slot={}  cmd={cmd}", node_source.slot);
        self.queue.lock().unwrap().push(BatchEntry { node_source, cmd, args });
        Ok(Value("(queued)".to_string()))
    }

    fn is_eval_cache_active(&self) -> bool { false }
    fn is_batch(&self) -> bool { true }
}

// ─────────────────────────────────────────────────────────────
// 桩实现
// ─────────────────────────────────────────────────────────────

struct StubConnectionManager;
impl ConnectionManager for StubConnectionManager {
    fn calc_slot(&self, key: &[u8]) -> u16 {
        key.iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16)) % 16384
    }
    fn use_script_cache(&self) -> bool { true }
}

// ─────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn_mgr: Arc<dyn ConnectionManager> = Arc::new(StubConnectionManager);
    let service = Arc::new(CommandAsyncService::new(conn_mgr));

    println!("=== CommandAsyncService（直接执行）===");
    {
        let codec: Arc<dyn Codec> = Arc::new(StringCodec);
        service.write_async(
            "lock:foo".into(), codec.clone(), "SET",
            vec![Value("val".into()), Value("PX".into()), Value("30000".into())],
        ).await?;
        service.read_async("lock:foo".into(), codec.clone(), "GET", vec![]).await?;
        service.eval_write_async(
            "lock:foo".into(), codec.clone(), "EVAL",
            "return redis.call('SET',KEYS[1],ARGV[1])".into(),
            vec!["lock:foo".into()],
            vec![Value("1".into())],
        ).await?;
        // is_batch / is_eval_cache_active 是内部方法（CommandDispatch），通过具体类型调用
        println!("is_batch         = {}", service.is_batch());
        println!("is_eval_cache    = {}", service.is_eval_cache_active());
        // is_track_changes 是公开接口（CommandAsyncExecutor），可通过 trait 约束调用
        println!("is_track_changes = {}", service.is_track_changes());
        println!("encode           = {:?}", service.encode(&StringCodec, Value("hi".into())).0);
    }

    println!();
    println!("=== CommandBatchService（缓冲后批量执行）===");
    {
        let batch = CommandBatchService::new(service.clone());
        let codec: Arc<dyn Codec> = Arc::new(StringCodec);

        batch.write_async("key:1".into(), codec.clone(), "SET", vec![Value("a".into())]).await?;
        batch.write_async("key:2".into(), codec.clone(), "SET", vec![Value("b".into())]).await?;
        batch.read_async("key:1".into(), codec.clone(), "GET", vec![]).await?;
        batch.eval_write_async(
            "lock:bar".into(), codec.clone(), "EVAL",
            "return redis.call('SET',KEYS[1],ARGV[1])".into(),
            vec!["lock:bar".into()],
            vec![Value("1".into())],
        ).await?;

        // 内部方法，通过具体类型（CommandDispatch）调用
        println!("is_batch         = {}", batch.is_batch());
        println!("is_eval_cache    = {}", batch.is_eval_cache_active());
        // 公开接口方法，通过 CommandAsyncExecutor 调用
        println!("is_track_changes = {}", batch.is_track_changes());

        println!();
        println!("--- execute_async() 触发真正执行 ---");
        let result = batch.execute_async().await?;
        println!("BatchResult: {} responses", result.responses.len());
        for (i, v) in result.responses.iter().enumerate() {
            println!("  [{i}] = {}", v.0);
        }
    }

    println!();
    // async fn in trait 不支持 dyn，改用泛型——这也是更地道的 Rust 写法
    println!("=== 泛型多态调用 ===");
    // show_info 只能调用 CommandAsyncExecutor 上的公开方法
    // is_batch / is_eval_cache_active 不在约束里，这里体现"接口隔离"
    async fn show_info<E: CommandAsyncExecutor>(name: &str, exec: &E) {
        println!("{name}: is_track_changes={}", exec.is_track_changes());
        exec.write_async_no_codec("poly:key".into(), "SET", vec![Value("v".into())]).await.unwrap();
    }

    show_info("CommandAsyncService", &*service).await;
    show_info("CommandBatchService", &CommandBatchService::new(service.clone())).await;

    Ok(())
}
