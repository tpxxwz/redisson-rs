# MasterSlaveEntry 职责分析与 fred 实现方案

## MasterSlaveEntry 是什么

Java 中 `MasterSlaveEntry` 是**一组主从节点的连接池管理器**，代表"一个 master + 若干 slave"这个组合单元。它的核心工作是：

1. **借连接**：从连接池里取出一条空闲的 `RedisConnection`，交给上层命令执行器去发命令
2. **还连接**：命令执行完后把 `RedisConnection` 归还连接池
3. **管理节点状态**：slave 下线/上线、master 切换（Sentinel 场景）
4. **提供 Pub/Sub 连接**：给 `PublishSubscribeService` 取订阅专用连接

每条 Redis 命令的执行链路是：

```
CommandAsyncService
  → getWriteEntry(slot) / getReadEntry(slot)   ← 拿到 MasterSlaveEntry
  → entry.connectionWriteOp(command)           ← 从连接池借一条连接
  → connection.async(command, args)            ← 在这条连接上发命令
  → entry.releaseWrite(connection)             ← 归还连接
```

---

## 方法分类

### 第一类：连接借还（核心热路径）

| Java 方法 | 作用 |
|---|---|
| `connectionWriteOp(command)` | 从 master 连接池借一条写连接 |
| `trackedConnectionWriteOp(command)` | 借写连接，同时开启 CLIENT TRACKING（用于 client-side cache） |
| `connectionReadOp(command, trackChanges)` | 根据 ReadMode 从 slave 或 master 借读连接 |
| `connectionReadOp(command, addr)` | 借指定地址节点的读连接（重定向场景） |
| `connectionReadOp(command, client, trackChanges)` | 借指定 RedisClient 节点的读连接 |
| `redirectedConnectionWriteOp(command, addr)` | MOVED/ASK 重定向时借目标节点的写连接 |
| `releaseWrite(connection)` | 归还写连接到 master 连接池 |
| `releaseRead(connection)` | 归还读连接到对应 slave/master 连接池 |

**fred 能做吗？完全不需要实现。**

这整个借还流程是 Redisson 自己管理 Netty Channel 的产物。fred Pool 内部已经封装了连接池管理，调用方直接 `pool.get("key").await` 即可，fred 自动选连接、自动归还。Rust 侧的 `CommandAsyncService` 不需要经过 `MasterSlaveEntry` 去拿连接，直接持有 `Arc<Pool>` 发命令即可。

---

### 第二类：Pub/Sub 连接管理

| Java 方法 | 作用 |
|---|---|
| `nextPubSubConnection(entry)` | 从 slave 或 master 的 PubSub 连接池取一条订阅连接 |
| `returnPubSubConnection(connection)` | 归还 PubSub 连接 |

**fred 能做吗？完全不需要实现。**

Rust 侧 `PublishSubscribeService` 直接持有 fred 的 `SubscriberClient`，订阅连接由 fred 统一管理，不需要通过 `MasterSlaveEntry` 获取。

---

### 第三类：节点元信息查询（对外暴露的只读接口）

| Java 方法 | 作用 | 外部调用方 |
|---|---|---|
| `getClient()` | 返回 master 的 `RedisClient`（用于拿地址、判断是否 master） | `ConnectionPool`、`RedissonBaseNodes`、`PublishSubscribeService` |
| `getAllEntries()` | 返回所有 `ClientConnectionsEntry`（master + slaves） | `CommandAsyncService`、`ConnectionPool`、`RedissonBaseNodes` |
| `isInit()` | 是否已完成初始化 | `MasterSlaveConnectionManager` 内部 |

**fred 能做吗？能，需要简化实现。**

- `getClient()`：Rust 侧返回代表 master 节点的 `Server`（fred 的 `Server { host, port }`）即可，不需要返回真实连接对象。
- `getAllEntries()`：返回该 entry 下所有节点的 `Server` 列表。在 Cluster 模式下，从 `with_cluster_state(|s| s.replicas(primary))` 获取；非 Cluster 模式下只有一个节点。

---

### 第四类：节点状态管理（Sentinel/Cluster 动态运维）

| Java 方法 | 作用 |
|---|---|
| `addSlave(address)` | 新增 slave 节点并初始化连接池 |
| `slaveDown(address)` | 标记 slave 下线，冻结连接池 |
| `slaveUp(address)` | slave 恢复，解冻连接池并重建连接 |
| `changeMaster(address)` | master 切换（Sentinel 主从切换时调用） |
| `excludeMasterFromSlaves(address)` | 将 master 从 slave 读池中移除 |
| `shutdownAsync()` | 关闭所有连接池 |

**fred 能做吗？不需要实现，fred 内部处理。**

fred 自带重连策略（`ReconnectPolicy`）和 Sentinel 主节点切换逻辑，节点下线/恢复/master 切换全部在 fred 内部自动完成，不需要 Rust 侧手动管理。

---

### 第五类：引用计数（Cluster slot 迁移用）

| Java 方法 | 作用 |
|---|---|
| `incReference()` / `decReference()` / `getReferences()` | 引用计数，Cluster slot 迁移时跟踪该 entry 还被几个 slot 引用 |

**fred 能做吗？不需要实现。**

fred 的 Cluster 模式自动处理 slot 迁移（MOVED/ASK 重定向），不需要 Rust 侧维护 slot 引用计数。

---

## Rust 侧 MasterSlaveEntry 的正确定位

**Java 中是连接池管理器，Rust 中应该是轻量节点描述符。**

Java 的 `MasterSlaveEntry` 之所以重，是因为它要自己管理 Netty Channel 的生命周期。Rust 侧 fred 已经接管了这一切，`MasterSlaveEntry` 只需要保留**对外暴露节点身份信息**的职责。

Rust 侧 `MasterSlaveEntry` 需要实现的字段和方法：

```rust
pub struct MasterSlaveEntry {
    /// master 节点标识（fred Server = host + port）
    pub master: fred::types::Server,
    /// slave 节点列表（Cluster 模式下从 ClusterRouting 缓存，非 Cluster 模式为空）
    pub slaves: Vec<fred::types::Server>,
    /// 反向引用，用于按节点发命令（with_cluster_node）
    pool: Arc<fred::clients::Pool>,
}

impl MasterSlaveEntry {
    /// 对应 getClient()：返回 master 节点标识
    pub fn get_client(&self) -> &fred::types::Server { &self.master }

    /// 对应 getAllEntries()：返回所有节点标识（master + slaves）
    pub fn all_servers(&self) -> Vec<&fred::types::Server> { ... }

    /// 向该 entry 的 master 节点发命令（绕过 Pool 的轮询，直接路由到指定节点）
    pub fn pool_for_master(&self) -> fred::clients::Client {
        self.pool.next().with_cluster_node(&self.master)
    }
}
```

`FredConnectionManager` 在 `init()` 完成后，按节点构建一份 `HashMap<Server, Arc<MasterSlaveEntry>>` 缓存，`get_write_entry(slot)` 和 `get_entry_set()` 都直接查这张表，热路径上零分配。

---

## 总结

| 方法类别 | Java 中的复杂度 | Rust 侧方案 |
|---|---|---|
| 连接借还（`connectionWriteOp/releaseWrite` 等） | 核心，几百行连接池逻辑 | **不需要**，fred Pool 内部处理 |
| PubSub 连接管理 | 复杂，分 master/slave 两套池 | **不需要**，`SubscriberClient` 处理 |
| 节点元信息（`getClient/getAllEntries`） | 简单 getter | **需要**，轻量实现 |
| 节点状态管理（slave down/up/changeMaster） | 重，涉及冻结/解冻/重建连接 | **不需要**，fred 内部自动处理 |
| 引用计数 | Cluster slot 迁移用 | **不需要**，fred 处理 MOVED/ASK |
