# ConnectionManager 接口方法的外部调用分析

分析目标：`ConnectionManager` 接口的每个方法，被哪些**非 ConnectionManager 实现类**的外部类调用。
排除范围：`MasterSlaveConnectionManager`、`SingleConnectionManager`、`SentinelConnectionManager`、`ClusterConnectionManager`、`ReplicatedConnectionManager`。

---

## 方法调用汇总

| 方法 | 外部调用类 | Rust 侧实现状态 |
|---|---|---|
| `getServiceManager()` | `CommandAsyncService`、`Redisson`、`CommandBatchService`、`RedisExecutor`、`PublishSubscribeService` | 已实现 |
| `getSubscribeService()` | `PublishSubscribeService`、`RedissonKeys`、`RedissonMapCache`、`RedissonPatternTopic`、`RedissonPermitExpirableSemaphore`、`RedissonLiveObjectService` | 已实现 |
| `calcSlot()` (三个重载) | `CommandAsyncService`、`PublishSubscribeService`、`RedissonObject`、`RenewalTask` | 已实现 |
| `getWriteEntry(int slot)` | `CommandAsyncService`、`CommandBatchService`、`RedisQueuedBatchExecutor`、`RedisExecutor`、`PublishSubscribeService` | 未实现（`unimplemented!()`） |
| `getReadEntry(int slot)` | `RedisExecutor` | 未实现（`unimplemented!()`） |
| `getEntrySet()` | `CommandAsyncService`、`PublishSubscribeService`、`RedissonKeys`、`RedissonNode` | 未实现（`unimplemented!()`） |
| `getEntry(String name)` | `PublishSubscribeService` | 未实现（`unimplemented!()`） |
| `getEntry(RedisClient)` | `CommandAsyncService`、`PublishSubscribeService` | 未实现（`unimplemented!()`） |
| `getEntry(InetSocketAddress)` | `DNSMonitor` | 未实现（`unimplemented!()`） |
| `createClient(NodeType, RedisURI, String)` | `MasterSlaveEntry` | 未实现（`unimplemented!()`） |
| `createClient(NodeType, InetSocketAddress, RedisURI, String)` | `MasterSlaveEntry` | 未实现（`unimplemented!()`） |
| `createCommandExecutor(...)` | `Redisson` | 未实现（`unimplemented!()`） |
| `shutdown()` | `Redisson` | 已实现 |
| `shutdown(quietPeriod, timeout)` | `Redisson` | 未实现（`unimplemented!()`） |
| `connect()` | `ConnectionManager.create()` 静态方法（非懒加载时调用） | 未实现（`unimplemented!()`） |
| `getLastClusterNode()` | **无外部调用** | 无需实现 |
| `getEntry(int slot)` | **无直接外部调用**（仅在实现类内部链式调用） | 无需实现 |
| `getEntry(RedisURI)` | **无直接外部调用** | 无需实现 |

---

## 按优先级分组

### 高优先级（核心命令链路直接依赖）

---

**`getWriteEntry(int slot)` / `getReadEntry(int slot)`**

被 `CommandAsyncService`、`RedisExecutor`、`CommandBatchService` 调用，是每条 Redis 命令执行时路由到正确节点的关键路径。没有这两个方法，所有 Redis 命令无法正常分发。

**fred 能做吗？** 能，但需要访问内部 API。

fred 的 `ClusterRouting`（`fred/src/protocol/types.rs`）提供了 `get_server(slot: u16) -> Option<&Server>` 返回主节点，以及启用 `replicas` feature 后的 `replicas(primary) -> Vec<Server>` 返回副本列表。访问路径是通过 `pool.next().inner().with_cluster_state(|state| state.get_server(slot))`。

**方案**：

- `get_write_entry(slot)`：通过 `with_cluster_state` 拿到 `get_server(slot)` 返回的 `Server`，包装成 `Arc<MasterSlaveEntry>` 返回。非 Cluster 模式下直接返回唯一的那个 entry。
- `get_read_entry(slot)`：在 `use_replica_for_reads = true` 且 Cluster 模式时，取 `replicas(primary)` 中随机一个副本节点对应的 entry；否则退化为与 `get_write_entry` 相同。
- 关键点：`MasterSlaveEntry` 需要在 `FredConnectionManager` 初始化时按节点预建好并缓存（`HashMap<Server, Arc<MasterSlaveEntry>>`），`get_write/read_entry` 只做查表，不在热路径上动态创建。

---

**`getEntrySet()`**

被 `CommandAsyncService`（遍历全节点执行）、`PublishSubscribeService`（Pub/Sub 重订阅）、`RedissonKeys`（全节点 SCAN）调用，影响面广。

**fred 能做吗？** 能。

fred 提供：
- `pool.clients()` 返回 `&[Client]`（Pool 中所有客户端）
- `with_cluster_state(|state| state.unique_primary_nodes())` 返回 `Vec<Server>`（Cluster 模式下所有主节点）
- 非 Cluster 模式只有一个节点，`pool.next().active_connections()` 可获取

**方案**：

`get_entry_set()` 直接返回缓存的 `Vec<Arc<MasterSlaveEntry>>`。与 `get_write/read_entry` 共用同一份按节点预建的 entry 缓存，在 `FredConnectionManager::init()` 完成后构建一次，之后只读。

---

**`createCommandExecutor(...)`**

被 `Redisson` 在初始化时调用，决定了整个命令执行器的构建，是启动链路的必要环节。

**fred 能做吗？** fred 的 `Pool` 本身就是命令执行器，实现了全套 Redis 命令 trait（`HashesInterface`、`KeysInterface`、`SortedSetsInterface` 等）。

**方案**：

`create_command_executor()` 返回一个 `CommandAsyncExecutor` 的具体实现（`CommandAsyncService`），其内部持有 `Arc<dyn ConnectionManager>`（即当前的 `FredConnectionManager`）。fred `Pool` 作为实际的命令发送层被 `FredConnectionManager` 封装持有，`CommandAsyncService` 通过 `connection_manager.pool` 调用 fred。

---

### 中优先级（连接管理与订阅）

---

**`getEntry(String name)` / `getEntry(RedisClient)`**

被 `PublishSubscribeService` 在订阅连接路由时调用，影响锁、信号量等所有依赖 Pub/Sub 通知机制的功能。

**fred 能做吗？** 部分能。

- `getEntry(String name)`：本质是 `calcSlot(name)` 再 `getWriteEntry(slot)`，fred 提供 `redis_keyslot(key)` 完成 slot 计算，然后查表即可。
- `getEntry(RedisClient)`：Java 中用于从一个已有连接反查其所属的节点组。fred 的设计里 Pool 中所有 Client 是等价轮询的，没有"某个 Client 归属于某个节点"的概念。Rust 侧的 `RedisClient` 是我们自己的包装类型，可以在其中额外持有一个 `Server` 字段记录归属节点，从而支持反查。

**方案**：

- `get_entry_by_name(name)`：`let slot = fred::util::redis_keyslot(name.as_bytes()); self.get_write_entry(slot)`
- `get_entry_by_client(client)`：在 `RedisClient` 包装类型中加一个 `server: Option<Server>` 字段，`get_entry_by_client` 取出该字段后查 entry 缓存表。

---

**`createClient(NodeType, RedisURI, String)` / `createClient(NodeType, InetSocketAddress, RedisURI, String)`**

被 `MasterSlaveEntry` 在初始化连接时调用。Java 中每次需要向新节点建连时动态创建 `RedisClient`；Rust 侧 fred Pool 在初始化时已经统一建好了所有连接。

**fred 能做吗？** fred 提供 `Client::new(config, ...)` 和 `Builder` 可以创建任意单节点客户端。

**方案**：

`create_client()` 用传入的 `RedisURI` / `InetSocketAddress` 构造 fred `Config`，调用 `Client::new()` 返回包装成 `RedisClient` 的实例。这个方法在 Rust 侧主要服务于 Cluster 动态扩缩容或 Sentinel 主节点切换时补充连接，当前阶段可以先实现为返回一个按地址新建的 fred `Client` 包装，不接入 Pool。

---

**`shutdown(quietPeriod, timeout)`**

被 `Redisson` 在带超时的优雅关闭时调用，是 `shutdown()` 的带参重载。

**fred 能做吗？** fred 的 `Pool::quit()` 不支持超时参数，只有无参版本。

**方案**：

用 `tokio::time::timeout(timeout, self.pool.quit())` 包裹，超时后忽略错误直接返回。`quiet_period` 对应 Netty EventLoop 的静默期概念，在 Tokio 模型下没有直接对应物，可以忽略或用 `tokio::time::sleep(quiet_period).await` 在 quit 前等待一段时间。

---

### 低优先级（非核心路径）

---

**`connect()`**

仅在 `ConnectionManager.create()` 的非懒加载分支调用一次，Rust 侧初始化已在 `FredConnectionManager::init()` 中完成，此方法作为 trait 方法实际上不会被调用到。

**方案**：保持 `unimplemented!()`，或改为 `// no-op：初始化已在 FredConnectionManager::init() 完成` 的空实现。

---

**`getEntry(InetSocketAddress)`**

仅被 `DNSMonitor` 调用，DNS 监控在 Rust 侧由 fred 内部处理。

**方案**：保持 `unimplemented!()`，Rust 侧不需要实现 `DNSMonitor`，此方法不会被调用。

---

### 无需实现

- **`getLastClusterNode()`**：无任何外部调用
- **`getEntry(int slot)`**：仅在实现类内部链式调用，不被外部直接使用
- **`getEntry(RedisURI)`**：无直接外部调用
