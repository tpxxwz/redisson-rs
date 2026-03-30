# MasterSlaveConnectionManager 笔记

这份文档记录 Java 版 `MasterSlaveConnectionManager` 的角色、字段和主流程，供 `redisson-rs` 后续对照实现。

不修改原有 `README.md`，这里只做实现导向的补充说明。

## 类层次

Java 这边的关系是：

- [`ConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ConnectionManager.java) 是接口
- [`MasterSlaveConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/MasterSlaveConnectionManager.java) 是核心基类实现
- 其上再派生出 4 个主要子类：
  - [`SingleConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/SingleConnectionManager.java)
  - [`ClusterConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ClusterConnectionManager.java)
  - [`SentinelConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/SentinelConnectionManager.java)
  - [`ReplicatedConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ReplicatedConnectionManager.java)

虽然名字叫 `MasterSlaveConnectionManager`，但它实际是各种部署模式共用的连接管理骨架。

## 这个类的定位

可以把它理解成：

- 负责通用连接启动流程
- 负责创建 `ServiceManager`
- 负责创建 `PublishSubscribeService`
- 负责创建和管理 `MasterSlaveEntry`
- 负责创建底层 `RedisClient`
- 负责 shutdown 和公共资源回收

更具体地说：

- 单机模式直接用它的默认逻辑
- cluster/sentinel/replicated 在它提供的骨架之上扩展各自特有的拓扑管理

## 主要字段在做什么

文件：
[`MasterSlaveConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/MasterSlaveConnectionManager.java)

### `MAX_SLOT`

- 常量 `16384`
- 表示 Redis cluster 的槽总数

### `singleSlotRange`

- `ClusterSlotRange(0, 16383)`
- 对非 cluster 模式来说，整个实例被视为覆盖一个“单一的大 slot 范围”

### `dnsMonitor`

- `DNSMonitor`
- 用于做 DNS 变化监控
- 主要服务于通过主机名连接时的地址变化场景

### `config`

- `MasterSlaveServersConfig`
- 是当前连接管理器真正使用的主从配置对象

### `masterSlaveEntry`

- `MasterSlaveEntry`
- 这是最核心的连接入口对象
- 在非 cluster 模式下，几乎所有读写最终都走它

### `subscribeService`

- `PublishSubscribeService`
- 负责 pub/sub 的订阅管理

### `serviceManager`

- `ServiceManager`
- 负责全局运行时基础设施，例如 timer、resolver、executor、watcher、renewal scheduler 等

### `nodeConnections`

- `Map<RedisURI, RedisConnection>`
- 缓存到具体节点的直连连接
- 主要用于节点探测、DNS 处理、cluster/sentinel 相关辅助连接等

### `lazyConnectLatch`

- `AtomicReference<CompletableFuture<Void>>`
- 用于 lazy initialization
- 保证多线程下只会有一个初始化流程真正执行

### `lastAttempt`

- 标记当前是否已经处于最后一次重试
- 主要用于失败后的 `internalShutdown()` 判断

## 主要逻辑在做什么

## 1. 构造阶段

构造函数里做的事情很重要：

1. 把外部配置归一化为 `MasterSlaveServersConfig`
2. 创建 `ServiceManager`
3. 创建 `PublishSubscribeService`

也就是说，从这个类开始，Redisson 的连接运行时骨架就搭起来了。

## 2. 节点连接缓存与节点直连

这部分由这些方法负责：

- `connectToNode(...)`
- `closeNodeConnections()`
- `closeNodeConnection(...)`
- `disconnectNode(...)`

作用：

- 为某个具体节点建立临时或辅助连接
- 连接可复用
- 连接失效时会关闭并移除缓存

这一层不等同于最终业务路由，它更像“连接管理器手里的节点工具连接”。

## 3. lazy 初始化

相关方法：

- `lazyConnect()`
- `isInitialized()`

作用：

- 如果配置是 lazy initialization，不在构造时立刻建连接
- 第一次真正需要 entry 时再触发连接
- 通过 `lazyConnectLatch` 保证并发安全

## 4. connect 重试主流程

相关方法：

- `connect()`
- `doConnect(...)`

`connect()` 负责：

- 按 `retryAttempts + 1` 做连接重试
- 失败时按 `retryDelay` sleep 后重试
- 最终失败时抛出异常

`doConnect(...)` 负责：

1. 根据配置决定创建 `SingleEntry` 还是 `MasterSlaveEntry`
2. 建立 master 连接
3. 如果有 slave，则初始化 slave balancer
4. 启动 DNS 监控

这里其实就是所有部署模式共享的启动骨架。

## 5. 运行时 cluster 探测

相关方法：

- `detectCluster()`

它会：

1. 从 `masterSlaveEntry` 拿一个读连接
2. 发一个跨两个 key 的 Lua 脚本
3. 如果服务端返回 `CROSSSLOT`
4. 则调用 `serviceManager.setClusterDetected(true)`

这意味着：

- 即使当前并不是显式的 cluster manager
- Java 版也会做一次运行时探测，判断后端是不是 cluster

这也是为什么 `ServiceManager` 里会有：

- `setClusterDetected`
- `isClusterSetup`

## 6. 创建底层 RedisClient

相关方法：

- `createClient(...)`
- `createRedisConfig(...)`

这部分负责把 `ServiceManager` 和配置里的运行时基础设施都灌进 `RedisClientConfig`：

- timer
- executor
- resolverGroup
- group
- socketChannelClass
- ssl 配置
- credentials
- command mapper
- connection events listener

也就是说，`MasterSlaveConnectionManager` 负责把：

- 配置层
- ServiceManager 提供的运行时资源
- RedisClient 实例化

这三者接起来。

## 7. 读写 entry 的默认实现

相关方法：

- `getEntry(...)`
- `getWriteEntry(int slot)`
- `getReadEntry(int slot)`
- `calcSlot(...)`

在这个基类里：

- `calcSlot(...)` 直接返回单一 slot
- `getEntry(...)` 基本直接返回 `masterSlaveEntry`

也就是说：

- 对单机/普通主从模式来说，路由非常简单
- 对 cluster 模式，这些方法会在子类里被扩展或覆写

## 8. changeMaster 骨架

相关方法：

- `changeMaster(int slot, RedisURI address)`

作用：

- 找到对应 `MasterSlaveEntry`
- 委托 entry 自己完成主节点切换

这说明主从切换的核心状态还是放在 `MasterSlaveEntry` 里，而 `ConnectionManager` 负责组织调用。

## 9. shutdown 与资源回收

相关方法：

- `shutdown()`
- `shutdown(long quietPeriod, long timeout, TimeUnit unit)`
- `internalShutdown()`

shutdown 流程会做：

1. 停止 DNS monitor
2. `serviceManager.close()`
3. 停止 `connectionWatcher`
4. 关闭 `resolverGroup`
5. 等待已有 future 收尾
6. shutdown 所有 `MasterSlaveEntry`
7. 关闭 executor
8. 停止 timer
9. 关闭 event loop group

说明：

- `MasterSlaveConnectionManager` 不只是“路由器”
- 它还是连接生命周期和运行时资源的总装配与总回收点

## 这个类和子类的分工

`MasterSlaveConnectionManager` 负责通用骨架：

- ServiceManager / subscribeService 初始化
- connect 重试流程
- RedisClient 创建
- MasterSlaveEntry 初始化
- lazy connect
- shutdown
- 基础 cluster 探测

子类负责特化：

### `SingleConnectionManager`

- 单机模式特有逻辑

### `ClusterConnectionManager`

- cluster 拓扑读取
- slot -> entry 路由
- cluster 变更检测
- 多个 `MasterSlaveEntry`

### `SentinelConnectionManager`

- sentinel 节点发现
- 根据 sentinel 确定当前 master

### `ReplicatedConnectionManager`

- replicated 模式下的节点发现和主从处理

## 对 `redisson-rs` 的实现含义

如果 Rust 后续继续依赖 `fred`，则 Java 里的很多底层职责不需要照抄：

- RedisClient 创建细节
- event loop / resolver / timer 接线
- cluster/sentinel 底层连接与路由

但这个类的“骨架职责”仍然值得参考：

1. 初始化共享运行时对象
2. 组织连接启动
3. 挂接 pubsub 和 command executor
4. 提供 shutdown 总入口
5. 把模式特化逻辑放到子类或模式分支里

## 参考文件

- [`MasterSlaveConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/MasterSlaveConnectionManager.java)
- [`ConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ConnectionManager.java)
- [`SingleConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/SingleConnectionManager.java)
- [`ClusterConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ClusterConnectionManager.java)
- [`SentinelConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/SentinelConnectionManager.java)
- [`ReplicatedConnectionManager.java`](/Users/wjj/projects/redisson/redisson/src/main/java/org/redisson/connection/ReplicatedConnectionManager.java)
