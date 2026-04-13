# fred `ClientInner` 结构体分析

> 文件路径：`fred/src/modules/inner.rs`

---

## 概览

`ClientInner` 是 fred 所有客户端类型（`Client`、`SubscriberClient`、`PooledClient` 等）的**共享核心状态**。

每个对外暴露的客户端对象内部持有 `RefCount<ClientInner>`（即 `Arc<ClientInner>`），多个客户端实例可以共享同一个 `ClientInner`（例如 `clone_new` 创建的 subscriber 会带走原 inner 的副本，但配置独立）。

`ClientInner` 本身是**不可克隆**的，但通过 `Arc` 共享引用。整个设计类似于 Java 中的"共享可变状态对象"，但借助 Rust 的并发原语保证线程安全。

---

## 类型别名说明

fred 通过 `runtime/_tokio.rs` 定义了一套可替换的类型别名，方便在 tokio / glommio 之间切换：

| 别名 | 实际类型 | 说明 |
|---|---|---|
| `RefCount<T>` | `Arc<T>` | 引用计数指针 |
| `RefSwap<T>` | `ArcSwapAny<T>` | 原子可替换的 Arc，无锁读取 |
| `RwLock<T>` | `parking_lot::RwLock<T>` | 同步读写锁 |
| `Mutex<T>` | `parking_lot::Mutex<T>` | 同步互斥锁 |
| `AsyncRwLock<T>` | `tokio::sync::RwLock<T>` | 异步读写锁 |
| `AtomicBool` | `std::sync::atomic::AtomicBool` | 原子布尔 |
| `AtomicUsize` | `std::sync::atomic::AtomicUsize` | 原子计数器 |
| `BroadcastSender<T>` | `tokio::sync::broadcast::Sender<T>` | 广播发送端 |
| `Sender<T>` / `Receiver<T>` | `tokio::sync::mpsc` 对应类型 | 命令通道 |

---

## `ClientInner` 字段详解

```rust
pub struct ClientInner {
    pub _lock:         Mutex<()>,
    pub id:            Str,
    pub resp3:         RefCount<AtomicBool>,
    pub state:         RwLock<ClientState>,
    pub config:        RefCount<Config>,
    pub connection:    RefCount<ConnectionConfig>,
    pub performance:   RefSwap<RefCount<PerformanceConfig>>,
    pub policy:        RwLock<Option<ReconnectPolicy>>,
    pub notifications: RefCount<Notifications>,
    pub counters:      ClientCounters,
    pub resolver:      AsyncRwLock<RefCount<dyn Resolve>>,
    pub backchannel:   RefCount<Backchannel>,
    pub server_state:  RwLock<ServerState>,

    pub command_tx:    RefSwap<RefCount<CommandSender>>,
    pub command_rx:    RwLock<Option<CommandReceiver>>,

    // 可选特性字段（见下文）
}
```

### 基础标识与协议

| 字段 | 类型 | 说明 |
|---|---|---|
| `_lock` | `Mutex<()>` | 内部同步锁，用于不允许并发的特定操作（如重连流程） |
| `id` | `Str` | 客户端唯一 ID，格式为 `"fred-<随机10字符>"`，用于日志和 `CLIENT SETNAME` |
| `resp3` | `RefCount<AtomicBool>` | 是否使用 RESP3 协议；`true` = RESP3，`false` = RESP2 |

### 状态与配置

| 字段 | 类型 | 说明 |
|---|---|---|
| `state` | `RwLock<ClientState>` | 客户端连接状态（`Disconnected` / `Connecting` / `Connected` / `Disconnecting`） |
| `config` | `RefCount<Config>` | 不可变配置（`Arc` 共享），包含 server 地址、认证、协议版本等 |
| `connection` | `RefCount<ConnectionConfig>` | 连接层配置，如超时、最大重试次数、命令缓冲区长度 |
| `performance` | `RefSwap<RefCount<PerformanceConfig>>` | 性能配置（广播容量、最大 feed count 等），可热更新（`ArcSwap` 无锁读） |
| `policy` | `RwLock<Option<ReconnectPolicy>>` | 重连策略（指数退避等）；`None` = 不自动重连 |

### 事件通知系统

```
notifications: RefCount<Notifications>
```

`Notifications` 是所有事件广播通道的集合，每个通道都是 `RefSwap<RefCount<BroadcastSender<T>>>`，可以被替换（用于关闭旧的接收端）。

| 通道 | 消息类型 | 对应接口 |
|---|---|---|
| `errors` | `(Error, Option<Server>)` | `on_error()` |
| `pubsub` | `Message` | `on_message()` — **这是 pub/sub 消息的分发出口** |
| `keyspace` | `KeyspaceEvent` | `on_keyspace_event()` |
| `reconnect` | `Server` | `on_reconnect()` |
| `cluster_change` | `Vec<ClusterStateChange>` | `on_cluster_change()` |
| `connect` | `Result<(), Error>` | `on_connect()` |
| `close` | `()` | 内部用，通知所有 task 关闭（`QUIT`/`SHUTDOWN`） |
| `unresponsive` | `Server` | 服务器无响应通知 |
| `invalidations` | `Invalidation` | `on_invalidation()`（feature `i-tracking`） |

**重要**：`pubsub` 通道就是 `SubscriberClient.message_rx()` 的底层来源。
所有 `SUBSCRIBE` / `PSUBSCRIBE` / `SSUBSCRIBE` 收到的消息，最终都通过 `notifications.broadcast_pubsub(message)` 分发。

### 命令路由通道

```
command_tx: RefSwap<RefCount<CommandSender>>   // mpsc sender → Router task
command_rx: RwLock<Option<CommandReceiver>>    // mpsc receiver，由 Router task 持有
```

这是 fred 的核心通信架构：

```
客户端 API 调用
    │
    ▼ try_send (非阻塞)
command_tx (mpsc Sender<RouterCommand>)
    │
    ▼
Router task (持有 command_rx)
    │
    ▼ 实际发送 Redis 协议命令到网络连接
```

- `command_rx` 正常情况下由 Router task `take()` 走，`ClientInner` 中为 `None`
- `has_command_rx()` 可判断 Router 是否已启动
- `swap_command_tx()` 用于重连时替换新的发送端
- 命令缓冲区满时，直接给调用方返回 `backpressure` 错误，不阻塞

### DNS 解析器

```
resolver: AsyncRwLock<RefCount<dyn Resolve>>
```

- 默认是 `DefaultResolver`（基于 `tokio::net::lookup_host`）
- 可通过 `set_resolver()` 替换为自定义实现（如 DNS-over-HTTPS）
- 用异步读写锁包裹，因为 `Resolve` trait 方法是 async 的

### Backchannel（后台管理通道）

```rust
pub struct Backchannel {
    pub transport:      AsyncRwLock<Option<ExclusiveConnection>>,
    pub blocked:        Mutex<Option<Server>>,
    pub connection_ids: Mutex<HashMap<Server, i64>>,
}
```

`Backchannel` 是一条**独立的 Redis 连接**，专门用于管理命令（如 `CLIENT UNBLOCK`、`CLUSTER INFO`、`DEBUG` 等），不经过命令路由通道：

- `transport`：独立的底层连接（`ExclusiveConnection`），与主连接池分开
- `blocked`：记录当前被 `BLPOP` 等命令阻塞的节点，`CLIENT UNBLOCK` 时使用
- `connection_ids`：`Server → connection_id` 映射，由 Router 维护；`active_connections()` 就是读这个

> 对应 Java Redisson 中的"后台管理连接"概念，但实现更轻量，只维护单条连接。

### 服务器状态缓存

```rust
pub server_state: RwLock<ServerState>

pub enum ServerKind {
    Sentinel { version, sentinels, primary },
    Cluster  { version, cache: Option<ClusterRouting> },
    Centralized { version },
}
```

- `ServerState` 根据配置类型持有对应的 `ServerKind`
- `ClusterRouting` 是集群槽位路由表，存储 slot → node 映射
- `num_cluster_nodes()` 从路由表中读取唯一主节点数量
- `with_cluster_state(fn)` 提供只读访问路由表的闭包接口

### 计数器

```rust
pub struct ClientCounters {
    pub cmd_buffer_len:   RefCount<AtomicUsize>,   // 当前缓冲区中的命令数
    pub redelivery_count: RefCount<AtomicUsize>,   // 重传命令次数
}
```

用于监控和背压判断。

### 可选特性字段

| 字段 | Feature | 说明 |
|---|---|---|
| `credentials_task` | `credential-provider` | 定时刷新认证信息的后台 task handle |
| `latency_stats` | `metrics` | 命令延迟滑动窗口统计 |
| `network_latency_stats` | `metrics` | 网络层延迟统计 |
| `req_size_stats` | `metrics` | 请求 payload 大小统计 |
| `res_size_stats` | `metrics` | 响应 payload 大小统计 |
| `last_command` | `dynamic-pool` | 最后一次命令时间戳（用于连接池空闲检测） |

---

## 关键方法分析

### `send_command()`

```rust
pub fn send_command(self: &RefCount<Self>, command: RouterCommand) -> Result<(), RouterCommand>
```

- 使用 `try_send`（**非阻塞**），缓冲区满时直接触发背压回调
- 背压时 `Command` 类型会通知调用方 `Err(backpressure)`；`Pipeline` / `Transaction` 类似处理
- 其他 `RouterCommand` 变体（如内部控制命令）在缓冲区满时返回 `Err(command)`，由上层处理

### `wait_with_interrupt()`

```rust
pub async fn wait_with_interrupt(&self, duration: Duration) -> Result<(), Error>
```

实现可中断的 sleep：

```
select {
    sleep(duration) => Ok(())
    notifications.close.recv() => Err(Canceled)
}
```

用于重连间隔等待，`broadcast_close()` 可打断所有等待中的 task。

### `cas_client_state()`

Compare-and-swap 状态机转换，防止并发状态竞争：

```rust
pub fn cas_client_state(&self, expected: ClientState, new_state: ClientState) -> bool
```

### `with_cluster_state()`

```rust
pub fn with_cluster_state<F, R>(&self, func: F) -> Result<R, Error>
where F: FnOnce(&ClusterRouting) -> Result<R, Error>
```

安全地访问集群路由表，如果不是集群模式或路由表未初始化，返回错误。

---

## 与 redisson-rs 的关联

在 redisson-rs 中，`PublishSubscribeService` 持有 `SubscriberClient`，后者内部持有 `Arc<ClientInner>`。

与 redisson-rs 直接相关的 `ClientInner` 字段：

| `ClientInner` 字段 | 在 redisson-rs 中的用途 |
|---|---|
| `notifications.pubsub` | `SubscriberClient.message_rx()` 的底层广播通道；消息分发 task 消费此通道，路由给 `pattern_listeners` |
| `server_state` | 判断集群模式、读取 `unique_primary_nodes()` 以便为每个主节点订阅 keyspace 通知 |
| `notifications.keyspace` | 可选：直接使用 fred 的 keyspace 事件通道，而非自行解析 pubsub 消息 |
| `notifications.reconnect` | 断线重连后可能需要重新触发某些逻辑（如锁续期重启） |
| `id` | 对应 Java 中 `CommandAsyncService` 的 `serviceManager.getId()`，用于生成 entry_name |

---

## 架构图

```
┌─────────────────────────────────────────────┐
│              ClientInner (Arc)              │
│                                             │
│  id, resp3, state, config, connection       │
│  performance, policy, resolver              │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │           Notifications             │   │
│  │  pubsub / keyspace / errors /       │   │
│  │  reconnect / connect / close / ...  │   │
│  └─────────────────────────────────────┘   │
│                                             │
│  ┌──────────────────────────────────────┐  │
│  │  command_tx  ──────►  Router task    │  │
│  │  command_rx  ◄──────  (holds rx)     │  │
│  └──────────────────────────────────────┘  │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │  server_state (ServerKind)          │   │
│  │  Sentinel | Cluster | Centralized   │   │
│  └─────────────────────────────────────┘   │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │  backchannel (独立管理连接)          │   │
│  │  transport / blocked / conn_ids     │   │
│  └─────────────────────────────────────┘   │
│                                             │
│  counters (cmd_buffer_len, redelivery)      │
└─────────────────────────────────────────────┘
```
