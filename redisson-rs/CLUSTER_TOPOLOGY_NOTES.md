# Java Cluster 拓扑处理笔记

这份文档记录 `redisson` Java 版在 cluster 模式下如何读取、组织、维护拓扑信息，供 `redisson-rs` 后续对照实现。

不修改原有 `README.md`，这里只做实现导向的补充说明。

## 总体流程

Java 版不是只依赖底层客户端做 cluster 路由，而是自己维护了一套显式的拓扑对象模型和索引结构。

整体流程如下：

1. 发送 `CLUSTER NODES`
2. 将返回结果解码成 `List<ClusterNodeInfo>`
3. 将节点视角的信息聚合成 `Collection<ClusterPartition>`
4. 再把 partition 映射成 `slot -> MasterSlaveEntry`
5. 命令执行时根据 slot 找到对应 `MasterSlaveEntry`
6. 同时维护旧拓扑索引，做 cluster 变更检测和错误诊断

## 关键类

### 1. `ClusterNodeInfo`

文件：
[`ClusterNodeInfo.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/cluster/ClusterNodeInfo.java)

职责：

- 表示 `CLUSTER NODES` 输出中的单个节点信息
- 更接近“节点视角”的原始拓扑对象

主要字段：

- `nodeId`
- `address`
- `flags`
- `slaveOf`
- `hostName`
- `slotRanges`
- `nodeInfo`

说明：

- `ClusterConnectionManager` 先通过 `ClusterNodesDecoder` 把 `CLUSTER NODES` 文本解码成 `List<ClusterNodeInfo>`
- 这一层还不是最终路由结构

### 2. `ClusterPartition`

文件：
[`ClusterPartition.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/cluster/ClusterPartition.java)

职责：

- 表示一个主分片及其从节点集合
- 是 Java 版 cluster 拓扑管理的核心逻辑对象

主要字段：

- `type`
- `nodeId`
- `masterFail`
- `masterAddress`
- `slaveAddresses`
- `failedSlaves`
- `slots`
- `slotRanges`
- `parent`
- `references`

说明：

- `ClusterNodeInfo` 是节点视角
- `ClusterPartition` 是分区/主从组视角
- `parsePartitions(nodes)` 会把多个 `ClusterNodeInfo` 聚合成 `ClusterPartition`

### 3. `MasterSlaveEntry`

文件：
[`MasterSlaveEntry.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/MasterSlaveEntry.java)

职责：

- 表示真正的连接管理与读写入口
- 一个 entry 对应一个主节点及其从节点连接

说明：

- `ClusterPartition` 偏拓扑逻辑
- `MasterSlaveEntry` 偏实际连接与路由执行

## `ClusterConnectionManager` 中的核心索引

文件：
[`ClusterConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ClusterConnectionManager.java)

### 1. `lastPartitions`

定义：

- `Map<Integer, ClusterPartition> lastPartitions`

作用：

- `slot -> ClusterPartition`
- 保存当前 slot 到 partition 的映射
- 用于 cluster 变更检测和覆盖检查

### 2. `lastUri2Partition`

定义：

- `Map<RedisURI, ClusterPartition> lastUri2Partition`

作用：

- `masterAddress -> ClusterPartition`
- 用于按地址反查 partition
- 也用于拓扑变更时定位已有分区

### 3. `slot2entry`

定义：

- `AtomicReferenceArray<MasterSlaveEntry> slot2entry`

作用：

- `slot -> MasterSlaveEntry`
- 这是命令路由最直接依赖的结构

### 4. `client2entry`

定义：

- `Map<RedisClient, MasterSlaveEntry> client2entry`

作用：

- `client -> MasterSlaveEntry`
- 用于按连接对象反查 entry

## 路由层次

Java 版 cluster 路由不是只有一层。

可以理解成：

1. 原始拓扑层：`ClusterNodeInfo`
2. 聚合拓扑层：`ClusterPartition`
3. 连接路由层：`MasterSlaveEntry`
4. 最终索引层：`slot2entry`

最终执行命令时，最关键的是：

- 先确定 slot
- 再通过 `slot2entry` 找到 `MasterSlaveEntry`
- 再由 `MasterSlaveEntry` 执行实际连接读写

## `parsePartitions` 在做什么

位置：

- [`ClusterConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ClusterConnectionManager.java)

主要工作：

1. 遍历 `List<ClusterNodeInfo>`
2. 跳过无地址、握手中、无 slot 的无效节点
3. 根据 `MASTER/SLAVE`、`slaveOf` 等信息，确定主从归属
4. 解析地址，必要时经过 `ServiceManager.resolveAll(...)`
5. 构造或补全 `ClusterPartition`
6. 为 master 记录：
   - `masterAddress`
   - `slotRanges`
   - `slots`
7. 为 slave 记录：
   - `slaveAddresses`
   - `failedSlaves`
8. 最终产出 `Collection<ClusterPartition>`

## Java 为什么要维护这么多结构

因为 Java 版不只是要“当前能路由”。

它还要处理：

- cluster 拓扑初始化
- slot 覆盖检查
- 节点新增/删除
- 主从切换
- cluster 拓扑 diff
- 更丰富的日志与异常诊断

所以它显式保留了：

- `ClusterNodeInfo`
- `ClusterPartition`
- `lastPartitions`
- `lastUri2Partition`
- `slot2entry`
- `client2entry`

## 对 `redisson-rs` 的实现含义

如果 Rust 后续要完全按 Java 版方式实现 cluster 管理，则至少需要考虑补齐以下层次：

1. `ClusterNodeInfo` 对应物
2. `ClusterPartition` 对应物
3. `slot -> partition` 映射
4. `slot -> entry` 映射
5. 拓扑刷新与 diff 逻辑

如果继续依赖 `fred` 的 `ClusterRouting` 作为主拓扑来源，则可以不完全照搬 Java 结构，但需要明确：

- 哪些 Java 语义直接由 `fred` 提供
- 哪些 Java 侧的诊断和状态缓存需要在 Rust 上层补一层

## `fred` 的 cluster 拓扑能力

本节基于本地 `fred 10.1.0` 源码整理。

结论先说：

- `fred` 不只是内部自己做 cluster 路由
- 它对外也暴露了缓存的 cluster 路由状态
- 能拿到主节点、slot、slot -> server 映射，以及 cluster 变化事件

### 1. 直接对外暴露的 cluster 接口

文件：
[`cluster.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/commands/interfaces/cluster.rs)

`ClusterInterface` 直接提供：

- `cached_cluster_state() -> Option<ClusterRouting>`
- `num_primary_cluster_nodes()`
- `sync_cluster()`
- `cluster_nodes()`
- `cluster_slots()`
- `cluster_info()`

说明：

- `cached_cluster_state()` 返回客户端当前用于路由的缓存 cluster state
- `cluster_nodes()` 和 `cluster_slots()` 也可以直接拿原始服务端视图

### 2. `ClusterRouting` 能提供什么

文件：
[`types.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/protocol/types.rs)

`ClusterRouting` 暴露了这些关键方法：

- `unique_hash_slots()`
- `unique_primary_nodes()`
- `hash_key(key)`
- `get_server(slot)`
- `replicas(primary)`（feature 打开时）

这意味着 `fred` 已经能直接回答：

- 当前有哪些 primary 节点
- 每个 primary 对应哪些代表性 slot
- 某个 key 的 hash slot 是多少
- 某个 slot 属于哪个 server
- 某个 primary 有哪些 replica

### 3. `fred` 会维护并刷新 cluster state

文件：
[`inner.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/modules/inner.rs)

关键点：

- `update_cluster_state(...)`
- `with_cluster_state(...)`
- `num_cluster_nodes()`

说明：

- `fred` 内部有一份缓存的 cluster routing state
- 上层可以通过公开接口拿到 clone 或基于该状态做查询

### 4. `fred` 暴露 cluster 变化事件

文件：
[`interfaces.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/interfaces.rs)

相关接口：

- `cluster_change_rx()`
- `on_cluster_change(...)`

说明：

- `fred` 不只是静态缓存 cluster state
- 它还会在 cluster 状态变化时广播事件

### 5. `fred` 内部路由也是基于 slot -> server

从源码可以看到，`fred` 的 router 在 cluster 模式下会通过：

- `command.cluster_hash()`
- `cache.get_server(slot)`

来定位目标节点。

也就是说，虽然它没有把 Java 的 `slot2entry` 这套类型原样暴露给你，但底层语义是同类的：

- 先算 slot
- 再用缓存的 cluster state 找对应 server
- 再把命令发到对应连接

## Java 结构与 `fred` 结构的对应关系

这两边不是一模一样的对象模型，但可以大致对应。

### 1. 原始拓扑输入层

Java：

- `CLUSTER NODES`
- `ClusterNodesDecoder`
- `List<ClusterNodeInfo>`

`fred`：

- `cluster_nodes()`
- `cluster_slots()`
- 内部主要基于 `CLUSTER SLOTS` 维护 `ClusterRouting`

说明：

- Java 更偏向从 `CLUSTER NODES` 构建自己的显式对象图
- `fred` 更偏向直接维护用于路由的 `ClusterRouting`

### 2. 聚合拓扑层

Java：

- `ClusterPartition`

`fred`：

- `ClusterRouting`

说明：

- `ClusterPartition` 是 Java 自己定义的“主分片 + 从节点 + slot 集合”对象
- `ClusterRouting` 是 `fred` 自己定义的“面向路由”的拓扑缓存
- 两者都在表达 cluster 的主节点、slot 和副本关系

### 3. slot 路由层

Java：

- `lastPartitions`
- `slot2entry`

`fred`：

- `ClusterRouting.get_server(slot)`

说明：

- Java 把 `slot -> partition` 和 `slot -> entry` 两层都显式存出来
- `fred` 更直接，用缓存 state 查 `slot -> server`

### 4. 连接执行层

Java：

- `MasterSlaveEntry`

`fred`：

- 内部 router / connections / pooled connections

说明：

- Java 的 `MasterSlaveEntry` 是显式连接管理对象
- `fred` 把这一层更多藏在自己的 router 和连接管理内部

### 5. 拓扑变化通知层

Java：

- `scheduleClusterChangeCheck(...)`
- `lastPartitions` / `lastUri2Partition` 做 diff

`fred`：

- `cluster_change_rx()`
- `on_cluster_change(...)`

说明：

- Java 自己做定时检查、自己做拓扑差分
- `fred` 已经有 cluster state 变化通知机制

## `redisson-rs` 后续实现时的建议对应方式

如果后续 Rust 继续以 `fred` 为底层主拓扑来源，可以按下面思路对应：

### 可以直接借 `fred`

- cluster state 缓存：`cached_cluster_state()`
- slot -> server 查询：`ClusterRouting.get_server(slot)`
- unique primary 节点：`unique_primary_nodes()`
- unique hash slot：`unique_hash_slots()`
- cluster 变化通知：`cluster_change_rx()`
- 原始 cluster 数据读取：`cluster_nodes()` / `cluster_slots()`

### 需要 Rust 上层自己决定要不要补一层

- Java 式的 `ClusterNodeInfo`
- Java 式的 `ClusterPartition`
- Java 式的 `slot -> entry`
- `lastClusterNodes` 这种诊断缓存
- `createNodeNotFoundException` 这种 Redisson 风格错误包装

### 判断原则

如果目标是：

- “先把行为跑通，依赖 `fred` 路由”

则优先直接用 `fred` 的 `ClusterRouting`。

如果目标是：

- “尽量复刻 Java Redisson 的结构和调试体验”

则需要在 `fred` 之上再补：

- `ClusterNodeInfo`
- `ClusterPartition`
- topology diff
- 诊断缓存与异常包装

## `fred` 是否足够支撑 Java 额外那一层

这里说的“Java 额外那一层”，指的是：

- 自己的一层对象模型
- 自己的一层索引结构
- 诊断信息缓存
- 异常包装

结论：

- `fred` 大多提供了实现这些能力所需的底层数据和事件
- 但它**没有直接提供 Java Redisson 这一层的对象模型和包装语义**
- 如果 `redisson-rs` 要复刻 Java 体验，这一层仍然要自己实现

### 1. 对象模型

Java 这边显式有：

- `ClusterNodeInfo`
- `ClusterPartition`
- `MasterSlaveEntry`

`fred` 这边提供的原材料有：

- `cached_cluster_state() -> ClusterRouting`
- `cluster_nodes()`
- `cluster_slots()`
- `unique_primary_nodes()`
- `unique_hash_slots()`
- `get_server(slot)`
- `replicas(primary)`

结论：

- 用这些数据足以支撑 Rust 自己构建 Java 风格的 cluster 对象模型
- 但 `ClusterPartition`、`ClusterNodeInfo` 这样的结构体需要 `redisson-rs` 自己定义

### 2. 索引结构

Java 显式维护：

- `slot -> partition`
- `masterAddress -> partition`
- `slot -> entry`
- `client -> entry`

`fred` 已经提供：

- `slot -> server`
- primary/replica 拓扑信息
- cluster 变化通知

结论：

- `slot -> partition`
- `masterAddress -> partition`

这类索引完全可以在 Rust 上层自己维护。

但：

- Java 的 `slot -> entry`

依赖的是 Java 自己的 `MasterSlaveEntry` 抽象。  
如果 Rust 也想保留这一层，就要自己再定义一层 entry 概念，不能直接等价拿 `fred` 内部对象来替代 Java `MasterSlaveEntry`。

### 3. 诊断信息

Java 还维护：

- `lastClusterNodes`
- cluster 相关诊断状态
- 更细的日志与错误上下文

`fred` 提供：

- `cluster_nodes()`
- `cluster_slots()`
- `cached_cluster_state()`
- `cluster_change_rx()`
- `error_rx()`
- `reconnect_rx()`

结论：

- 诊断原材料是有的
- `lastClusterNodes` 这类缓存也完全可以自己补
- 但 `fred` 不会自动替你维护 Java 风格的诊断文本与状态描述

### 4. 异常包装

Java 有：

- `createNodeNotFoundFuture(...)`
- `createNodeNotFoundException(...)`

本质是：

- 基于 Redisson 自己维护的 slot / node / topology 状态
- 组装成更贴近 Redisson 语义的异常

`fred` 会提供：

- 自己的 cluster/sentinel 错误
- MOVED / ASK 相关错误
- error 事件流

但 `fred` 不会直接提供：

- Java Redisson 风格的“Node for slot X hasn't been discovered yet. Last cluster nodes topology: ...”这一层异常文本和包装逻辑

结论：

- 底层错误有
- Java 风格异常包装没有
- 如果想保留这层体验，需要 `redisson-rs` 自己封装

## 总结

如果后续 `redisson-rs` 要实现 Java Redisson 额外那层：

- **对象模型**：`fred` 提供了足够的数据来源，但对象类要自己建
- **索引结构**：可以自己建，但 entry 抽象要自己补
- **诊断信息**：原材料有，但诊断缓存和展示方式要自己做
- **异常包装**：需要自己实现

一句话：

- `fred` 提供的是“底层拓扑状态、路由能力和事件”
- Java Redisson 额外那层提供的是“自己的对象模型、索引结构、诊断语义和异常表达”
- 前者 `fred` 大多有，后者 `redisson-rs` 需要自己补

## 参考文件

- [`ClusterConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ClusterConnectionManager.java)
- [`ClusterPartition.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/cluster/ClusterPartition.java)
- [`ClusterNodeInfo.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/cluster/ClusterNodeInfo.java)
- [`MasterSlaveEntry.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/MasterSlaveEntry.java)
- [`cluster.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/commands/interfaces/cluster.rs)
- [`types.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/protocol/types.rs)
- [`inner.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/modules/inner.rs)
- [`interfaces.rs`](/Users/wjj/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/fred-10.1.0/src/interfaces.rs)
