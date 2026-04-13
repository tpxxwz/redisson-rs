# MasterSlaveConnectionManager 内部类的外部引用分析

分析对象：`MasterSlaveConnectionManager` 内部使用的三个类——`MasterSlaveEntry`、`RedisConnection`、`DNSMonitor`——在整个 `org.redisson` 包中被哪些**外部类**（排除 `MasterSlaveConnectionManager` 本身及其子类）引用。

---

## DNSMonitor

**无任何外部类引用。**

`start()` / `stop()` 完全封装在 `MasterSlaveConnectionManager` 内部，不对外暴露。

**结论**：Rust 侧不需要在 trait 层暴露 DNSMonitor 相关接口，属于实现细节。

---

## MasterSlaveEntry

| 分类 | 外部类 | 调用的方法 |
|---|---|---|
| 命令执行（接口） | `CommandAsyncExecutor` | `readAsync / writeAsync / evalReadAsync / evalWriteAsync` 的参数类型 |
| 命令执行（实现） | `CommandAsyncService` | 同上；`getAllEntries()`、`getEntrySet()` 遍历 |
| 批量命令 | `CommandBatchService`、`RedisQueuedBatchExecutor` | 作为 `Map<MasterSlaveEntry, ...>` 的 key，按 entry 分组聚合命令 |
| 单条命令 | `RedisExecutor` | 字段 `entry`，驱动连接获取与命令发送 |
| 连接池 | `ConnectionPool` 及子类（Master/Slave/PubSub） | 构造参数；`getAllEntries()`、`getClient()` |
| 订阅 | `PublishSubscribeService`、`PubSubConnectionEntry` | 字段/参数；`getEntry()`、`getClient()`、`getAllEntries()` |
| 键操作 | `RedissonKeys`、`RedissonKeysReactive`、`RedissonKeysRx` | `getEntrySet()` 遍历 entry 执行 SCAN |
| 节点管理 | `RedissonNode`、`RedissonBaseNodes` | `connectionReadOp/WriteOp()`、`releaseRead/Write()`、`getClient()`、`getAllEntries()` |
| 事务 | `RedissonTransaction` | `getEntrySet().iterator().next()` 取第一个 entry |
| JCache | `JCache` | 作为内部操作方法的参数传递 |

---

## RedisConnection

| 分类 | 外部类 | 调用的方法 |
|---|---|---|
| 连接池基础 | `ConnectionsHolder<T extends RedisConnection>` | `isClosed()`、`getRedisClient()`、`setLastUsageTime()`、`decUsage()`、`closeAsync()` |
| 连接条目 | `ClientConnectionsEntry` | `closeAsync()`；作为 Map key |
| 空闲监控 | `IdleConnectionWatcher` | `getLastUsageTime()`、`closeIdleAsync()` |
| 命令执行 | `RedisExecutor`、`CommandBatchService` | `connectionFuture` 类型；发送命令、处理结果、释放连接 |
| 订阅 | `PublishSubscribeService` | `checkPatternSupport()`、`checkShardingSupport()` 参数 |
| 节点管理 | `RedissonNode`、`RedissonBaseNodes` | `getChannel()`；`connectAsync()` 后直接操作连接 |
| 客户端 handler | `RedisChannelInitializer`、`PingConnectionHandler` 等 | `getFrom(channel)`、`getRedisClient()`、`closeAsync()` |

---

## ClusterSlotRange

**定义位置：** `org.redisson.cluster.ClusterSlotRange`

不可变值对象，表示 Redis Cluster 中一段连续的 slot 范围（`startSlot`..`endSlot`）。

**核心方法：** `getStartSlot()`、`getEndSlot()`、`hasSlot(int)`、`size()`、`equals/hashCode`（用作 Map key）

### 外部引用者

| 分类 | 外部类 | 使用方式 |
|---|---|---|
| 集群拓扑解析 | `SlotsDecoder` | 解码 `CLUSTER SLOTS` 响应，构造 `Map<ClusterSlotRange, Set<String>>` |
| 集群拓扑解析 | `ClusterNodesDecoder` | 解码 `CLUSTER NODES` 响应，将文本格式的 slot 信息解析为 `ClusterSlotRange` 对象 |
| 节点信息 | `ClusterNodeInfo` | 存储节点的所有 slot 范围（`Set<ClusterSlotRange>`） |
| 分区管理 | `ClusterPartition` | 存储分区的 slot 范围；遍历 `getStartSlot/getEndSlot` 构建 BitSet 加速查询 |
| 连接管理 | `MasterSlaveConnectionManager` | 定义 `singleSlotRange = new ClusterSlotRange(0, 16383)`，非集群模式下代表全量 slot |
| 节点管理 API | `RedisClusterNode`（接口）、`RedisClusterNodeAsync`（接口） | 返回类型 `Map<ClusterSlotRange, Set<String>>` 暴露给应用层 |
| 节点管理实现 | `RedisNode` | 实现 `clusterSlots()` / `clusterSlotsAsync()`，调用 `SlotsDecoder` 解码结果 |

### 核心使用模式

```
CLUSTER SLOTS / CLUSTER NODES 命令
    → SlotsDecoder / ClusterNodesDecoder 解析
    → ClusterSlotRange 对象
    → 存入 ClusterNodeInfo / ClusterPartition
    → ClusterPartition 内部转为 BitSet 加速 hasSlot() 查询
    → 通过 RedisClusterNode API 向应用层暴露
```

**结论**：`ClusterSlotRange` 是集群拓扑管理的基础值类型，仅在 Cluster 模式下有实际意义。非集群模式下 `MasterSlaveConnectionManager` 用它持有一个固定的全量范围占位，不参与真正的路由计算。Rust 侧已通过 `fred::util::redis_keyslot` 替代路由计算，`ClusterSlotRange` 本身可以按需实现为简单的 `struct ClusterSlotRange { start: u16, end: u16 }`，优先级不高。

---

## 对 Rust 实现优先级的影响

- **DNSMonitor**：完全是 `MasterSlaveConnectionManager` 的内部细节，Rust 侧由 fred 自动处理重连与 DNS 解析，无需实现。
- **MasterSlaveEntry**：被命令执行、连接池、订阅、业务层广泛引用，是贯穿全局的核心类型，Rust 侧需要优先完善其对应实现。
- **RedisConnection**：被连接池、命令执行、客户端 handler 深度依赖，在 Rust 侧由 fred 的连接抽象承担，无需单独对标，但相关的连接生命周期管理（`ClientConnectionsEntry`、`ConnectionsHolder`）值得跟进实现。
