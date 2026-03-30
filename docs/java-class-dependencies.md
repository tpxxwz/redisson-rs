# Java Redisson 核心类依赖关系

> 本文档梳理 `org.redisson` 包下核心类的接口/实现关系、字段组成及依赖图。
> 对应 Rust 侧实现时可作为参考。

---

## 一、接口（trait 对应物）

### `RedissonClient`（org.redisson.api）
顶层客户端接口，`Redisson` 的对外门面。提供所有 Redis 对象的工厂方法（getBucket / getLock / getMap 等）。

---

### `ConnectionManager`（org.redisson.connection）
连接拓扑管理接口。负责节点路由、连接创建与关闭。

| 关键方法 | 说明 |
|---|---|
| `connect()` | 初始化连接 |
| `calcSlot(key)` | 计算 cluster slot |
| `getEntry(slot/name/addr/client)` | 查找 MasterSlaveEntry |
| `getWriteEntry / getReadEntry` | 读写路由 |
| `createClient(NodeType, ...)` | 创建底层 RedisClient |
| `getSubscribeService()` | 返回 PubSub 服务 |
| `getServiceManager()` | 返回 ServiceManager |
| `createCommandExecutor(...)` | 创建命令执行器 |
| `shutdown(...)` | 优雅关闭 |

**实现类**：见第二节。

---

### `CommandAsyncExecutor`（org.redisson.command）
命令异步执行接口。所有 Redis 命令的发出入口。

| 关键字段/方法 | 说明 |
|---|---|
| `getConnectionManager()` | 获取连接管理器 |
| `getServiceManager()` | 获取服务管理器 |
| `writeAsync(...)` | 写命令 |
| `readAsync(...)` | 读命令 |
| `evalWriteAsync(...)` | Lua 脚本写 |
| `evalReadAsync(...)` | Lua 脚本读 |
| `copy(ObjectParams)` | 创建带参数副本 |
| `isEvalShaROSupported()` | 是否支持 EVALSHA_RO |
| 内部枚举 `SyncMode` | AUTO / WAIT / WAIT_AOF |

**实现类**：见第二节。

---

## 二、实现类

### `MasterSlaveConnectionManager`（org.redisson.connection）
`ConnectionManager` 的主要实现，也是 Sentinel / Cluster / Replicated 等实现的基类。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `singleSlotRange` | `ClusterSlotRange` | 非 cluster 时固定 slot 范围（0~MAX） |
| `dnsMonitor` | `DNSMonitor` | DNS 动态解析监控 |
| `config` | `MasterSlaveServersConfig` | 本管理器对应的服务器配置 |
| `masterSlaveEntry` | `MasterSlaveEntry` | 当前主从节点组 |
| `subscribeService` | `PublishSubscribeService` | Pub/Sub 服务 |
| `serviceManager` | `ServiceManager` | 服务/定时器管理器 |
| `nodeConnections` | `Map<RedisURI, RedisConnection>` | 已建立的节点连接缓存 |
| `lazyConnectLatch` | `AtomicReference<CompletableFuture<Void>>` | 懒连接的 latch |

**依赖**：`MasterSlaveServersConfig`、`ServiceManager`、`PublishSubscribeService`、`MasterSlaveEntry`

---

### `CommandAsyncService`（org.redisson.command）
`CommandAsyncExecutor` 的主要实现。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `codec` | `Codec` | 序列化编解码器 |
| `connectionManager` | `ConnectionManager` | 连接管理器（路由） |
| `objectBuilder` | `RedissonObjectBuilder` | Live Object 对象构建器 |
| `referenceType` | `RedissonObjectBuilder.ReferenceType` | RxJava / Reactive / Default |
| `retryAttempts` | `int` | 命令重试次数 |
| `retryDelay` | `DelayStrategy` | 重试延迟策略 |
| `responseTimeout` | `int` | 命令超时（ms） |
| `trackChanges` | `boolean` | 是否追踪变更（用于事务） |

**构造函数三种重载**：
1. `(CommandAsyncExecutor, boolean trackChanges)` — 从已有 executor 拷贝
2. `(CommandAsyncExecutor, ObjectParams)` — 从已有 executor + 对象参数拷贝
3. `(ConnectionManager, RedissonObjectBuilder, ReferenceType)` — 全量构造

**依赖**：`ConnectionManager`、`RedissonObjectBuilder`、`Codec`、`DelayStrategy`

---

## 三、支撑类（无接口，直接使用具体类）

### `Config`（org.redisson.config）
顶层配置对象，由用户填写后传入 `Redisson.create(config)`。

**关键字段**（分组）：

| 分组 | 字段 | 类型 |
|---|---|---|
| 服务器模式（互斥） | `singleServerConfig` | `SingleServerConfig` |
| | `masterSlaveServersConfig` | `MasterSlaveServersConfig` |
| | `sentinelServersConfig` | `SentinelServersConfig` |
| | `clusterServersConfig` | `ClusterServersConfig` |
| | `replicatedServersConfig` | `ReplicatedServersConfig` |
| 认证 | `username` / `password` | `String` |
| | `credentialsResolver` | `CredentialsResolver` |
| 线程 | `threads` / `nettyThreads` | `int`（默认 16/32） |
| | `executor` / `nettyExecutor` | `ExecutorService` / `Executor` |
| 网络 | `transportMode` | `TransportMode`（NIO） |
| | `eventLoopGroup` | `EventLoopGroup` |
| | `protocol` | `Protocol`（RESP2） |
| 编解码 | `codec` | `Codec` |
| 锁 | `lockWatchdogTimeout` | `long`（30s） |
| | `lockWatchdogBatchSize` | `int`（100） |
| | `fairLockWaitTimeout` | `long`（5min） |
| | `checkLockSyncedSlaves` | `boolean` |
| 缓存清理 | `minCleanUpDelay` / `maxCleanUpDelay` | `int`（5s / 1800s） |
| | `cleanUpKeysAmount` | `int`（100） |
| 名称/命令映射 | `nameMapper` / `commandMapper` | `NameMapper` / `CommandMapper` |
| SSL | `sslProvider` / `sslVerificationMode` 等 | 多个 |
| 其他 | `lazyInitialization` | `boolean` |
| | `referenceEnabled` | `boolean`（true） |
| | `useScriptCache` | `boolean`（true） |

---

### `ServiceManager`（org.redisson.connection）
内部服务总线，持有定时器、线程池、事件循环等底层资源。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `cfg` | `Config` | 原始顶层配置 |
| `config` | `MasterSlaveServersConfig` | 当前服务器配置 |
| `group` | `EventLoopGroup` | Netty 事件循环组 |
| `socketChannelClass` | `Class<DuplexChannel>` | NIO/Epoll channel 类型 |
| `resolverGroup` | `AddressResolverGroup` | DNS 解析器组 |
| `executor` | `ExecutorService` | 通用线程池 |
| `timer` | `HashedWheelTimer` | 时间轮定时器（锁续期等） |
| `connectionWatcher` | `IdleConnectionWatcher` | 空闲连接清理 |
| `connectionEventsHub` | `ConnectionEventsHub` | 连接事件广播 |
| `elementsSubscribeService` | `ElementsSubscribeService` | 元素订阅服务 |
| `natMapper` | `NatMapper` | NAT 地址映射 |
| `id` | `String` | 实例唯一 ID |
| `shutdownLatch` | `AtomicBoolean` | 关闭状态标志 |

---

### `EvictionScheduler`（org.redisson.eviction）
定期清理带 TTL 的 Map/Cache 中过期条目。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `tasks` | `Map<String, EvictionTask>` | 每个对象名对应一个清理任务 |
| `executor` | `CommandAsyncExecutor` | 执行 Redis 清理命令 |

**依赖**：`CommandAsyncExecutor`

---

### `EvictionTask`（org.redisson.eviction，abstract）
单个清理任务，实现 `TimerTask`。自适应调整清理间隔。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `executor` | `CommandAsyncExecutor` | 执行 Redis 命令 |
| `sizeHistory` | `Deque<Integer>` | 历史清理量，用于自适应调速 |
| `minDelay` / `maxDelay` | `int` | 最小/最大间隔（s） |
| `keysLimit` | `int` | 单次最多清理 key 数 |
| `delay` | `int` | 当前间隔（初始 5s） |
| `timeout` | `Timeout`（volatile） | 当前定时句柄 |

**依赖**：`CommandAsyncExecutor`

---

### `WriteBehindService`（org.redisson）
Write-behind 异步写回服务，配合 `RMapCache` 的 write-behind 策略使用。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `tasks` | `ConcurrentMap<String, MapWriteBehindTask>` | 每个 map 名对应一个写回任务 |
| `executor` | `CommandAsyncExecutor` | 执行 Redis 命令 |

**依赖**：`CommandAsyncExecutor`

---

### `MasterSlaveEntry`（org.redisson.connection）
代表一组主从节点（1 master + N slaves），是连接路由的最小单元。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `masterEntry` | `ClientConnectionsEntry`（volatile） | 当前 master 节点的连接条目 |
| `replacedBy` | `MasterSlaveEntry` | 主从切换时指向接替的 entry |
| `references` | `int` | 引用计数（cluster 中多 slot 共享同一 entry） |
| `config` | `MasterSlaveServersConfig` | 主从配置 |
| `connectionManager` | `ConnectionManager` | 所属连接管理器（反向引用） |
| `masterConnectionPool` | `MasterConnectionPool` | master 命令连接池 |
| `masterPubSubConnectionPool` | `MasterPubSubConnectionPool` | master PubSub 连接池 |
| `slavePubSubConnectionPool` | `PubSubConnectionPool` | slave PubSub 连接池 |
| `slaveConnectionPool` | `SlaveConnectionPool` | slave 命令连接池 |
| `client2Entry` | `Map<RedisClient, ClientConnectionsEntry>` | 每个 RedisClient 对应的连接条目 |
| `active` | `AtomicBoolean` | 该 entry 是否仍活跃 |
| `noPubSubSlaves` | `AtomicBoolean` | 是否没有可用 PubSub slave |
| `availableSlaves` | `int`（volatile） | 可用 slave 数量（-1 表示未知） |
| `aofEnabled` | `boolean`（volatile） | 该节点是否开启了 AOF |

**`ClientConnectionsEntry` 字段**（单个节点的连接持有者）：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `client` | `RedisClient` | 对应的底层 Redis 客户端 |
| `nodeType` | `NodeType` | MASTER / SLAVE / SENTINEL |
| `connectionsHolder` | `ConnectionsHolder<RedisConnection>` | 普通命令连接池 |
| `pubSubConnectionsHolder` | `ConnectionsHolder<RedisPubSubConnection>` | PubSub 连接池 |
| `trackedConnectionsHolder` | `TrackedConnectionsHolder` | client-side caching 追踪连接池 |
| `freezeReason` | `FreezeReason`（volatile） | 节点被冻结的原因（MANAGER / RECONNECT） |
| `idleConnectionWatcher` | `IdleConnectionWatcher` | 空闲连接清理监视器 |
| `connectionManager` | `ConnectionManager` | 所属连接管理器 |
| `initialized` | `boolean`（volatile） | 是否已完成初始化 |
| `connection2holder` | `Map<RedisConnection, ConnectionsHolder<?>>` | 连接到其所在池的反向映射 |

**`ConnectionPool<T>` 字段**（所有连接池的抽象基类）：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `connectionManager` | `ConnectionManager` | 所属连接管理器 |
| `config` | `MasterSlaveServersConfig` | 主从配置 |
| `masterSlaveEntry` | `MasterSlaveEntry` | 所属的主从 entry（反向引用） |

子类：`MasterConnectionPool`、`MasterPubSubConnectionPool`、`SlaveConnectionPool`、`PubSubConnectionPool`（均无额外字段，差异在连接创建逻辑）

---

### `PublishSubscribeService`（org.redisson.pubsub）
管理所有 Pub/Sub 订阅连接，是锁、信号量、CountDownLatch 等通知机制的基础。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `connectionManager` | `ConnectionManager` | 连接管理器（用于获取订阅连接） |
| `config` | `MasterSlaveServersConfig` | 主从配置 |
| `locks` | `AsyncSemaphore[50]` | 按 channel hash 分桶的异步锁（防止并发订阅同一 channel） |
| `freePubSubLock` | `AsyncSemaphore(1)` | 保护空闲订阅连接查找的互斥锁 |
| `name2entry` | `Map<ChannelName, Collection<PubSubConnectionEntry>>` | channel 名 → 订阅连接条目 |
| `name2PubSubConnection` | `ConcurrentMap<PubSubKey, PubSubConnectionEntry>` | (channel+entry) → 订阅连接 |
| `entry2PubSubConnection` | `ConcurrentMap<MasterSlaveEntry, PubSubEntry>` | 节点 entry → 该节点上所有订阅连接 |
| `key2connection` | `Map<Tuple<ChannelName, ClientConnectionsEntry>, PubSubConnectionEntry>` | 细粒度的连接映射 |
| `lockPubSub` | `LockPubSub` | 分布式锁的 Pub/Sub 处理器 |
| `semaphorePubSub` | `SemaphorePubSub` | 信号量的 Pub/Sub 处理器 |
| `countDownLatchPubSub` | `CountDownLatchPubSub` | CountDownLatch 的 Pub/Sub 处理器 |
| `trackedEntries` | `Set<PubSubConnectionEntry>` | client-side caching 追踪订阅条目 |
| `shardingSupported` | `boolean` | 是否支持 sharded pub/sub（SPUBLISH） |
| `patternSupported` | `boolean` | 是否支持 pattern 订阅（PSUBSCRIBE） |

**内部类**：

- `PubSubKey`：`(ChannelName, MasterSlaveEntry)` 的组合键，用于 cluster 下区分不同 slot 的同名 channel
- `PubSubEntry`：持有同一节点上的多个 `PubSubConnectionEntry`（一个物理连接可承载多个订阅）

---

### `Redisson`（org.redisson）
实现 `RedissonClient`，是整个客户端的入口和组合根。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `config` | `Config` | 原始用户配置 |
| `connectionManager` | `ConnectionManager` | 连接管理器 |
| `commandExecutor` | `CommandAsyncExecutor` | 命令执行器 |
| `evictionScheduler` | `EvictionScheduler` | TTL 清理调度器 |
| `writeBehindService` | `WriteBehindService` | Write-behind 服务 |
| `liveObjectClassCache` | `ConcurrentMap<Class, Class>` | Live Object 代理类缓存 |

---

### `RedissonObject`（org.redisson，abstract）
所有 Redisson Redis 对象（RBucket、RLock、RMap 等）的抽象基类，实现 `RObject` 接口。

**字段**：

| 字段名 | 类型 | 说明 |
|---|---|---|
| `commandExecutor` | `CommandAsyncExecutor` | 命令执行器，所有 Redis 操作通过它发出 |
| `name` | `String` | 该对象在 Redis 中的 key 名称 |
| `codec` | `Codec`（final） | 序列化编解码器，由 `ServiceManager.getCodec(codec)` 解析 |
| `listeners` | `Map<String, Collection<Integer>>` | 已注册的事件监听器 ID（按 channel 分组） |

**构造函数**：
- `(Codec, CommandAsyncExecutor, String name)` — codec 由 ServiceManager 规范化后赋值
- `(CommandAsyncExecutor, String name)` — codec 从 `serviceManager.getCfg().getCodec()` 取默认值

**依赖**：`CommandAsyncExecutor`（进而间接依赖 `ConnectionManager`、`ServiceManager`、`Codec`）

子类继承链：
- `RedissonBucket → RedissonObject`
- `RedissonLock → RedissonBaseLock → RedissonExpirable → RedissonObject`
- `RedissonMap → RedissonObject`

---

## 四、依赖关系图

```
Redisson (implements RedissonClient)
  ├─ config: Config
  │     ├─ codec: Codec
  │     ├─ nameMapper: NameMapper
  │     ├─ commandMapper: CommandMapper
  │     ├─ lockWatchdogTimeout: long
  │     ├─ threads / nettyThreads: int
  │     └─ [MasterSlave|Sentinel|Cluster|Replicated|Single]ServersConfig (互斥，按模式选一)
  │
  ├─ connectionManager: MasterSlaveConnectionManager (implements ConnectionManager)
  │     ├─ config: MasterSlaveServersConfig              ← 来自 Config
  │     ├─ singleSlotRange: ClusterSlotRange
  │     ├─ dnsMonitor: DNSMonitor
  │     ├─ nodeConnections: Map<RedisURI, RedisConnection>
  │     │
  │     ├─ serviceManager: ServiceManager
  │     │     ├─ cfg: Config                             ← 同上
  │     │     ├─ config: MasterSlaveServersConfig        ← 同上
  │     │     ├─ group: EventLoopGroup
  │     │     ├─ executor: ExecutorService
  │     │     ├─ timer: HashedWheelTimer
  │     │     ├─ connectionWatcher: IdleConnectionWatcher
  │     │     ├─ connectionEventsHub: ConnectionEventsHub
  │     │     ├─ elementsSubscribeService: ElementsSubscribeService
  │     │     ├─ resolverGroup: AddressResolverGroup
  │     │     ├─ natMapper: NatMapper
  │     │     └─ id: String
  │     │
  │     ├─ masterSlaveEntry: MasterSlaveEntry
  │     │     ├─ config: MasterSlaveServersConfig        ← 同上
  │     │     ├─ connectionManager: ConnectionManager    ← 反向引用
  │     │     ├─ masterEntry: ClientConnectionsEntry
  │     │     │     ├─ client: RedisClient               (master 节点)
  │     │     │     ├─ nodeType: MASTER
  │     │     │     ├─ connectionsHolder: ConnectionsHolder<RedisConnection>
  │     │     │     ├─ pubSubConnectionsHolder: ConnectionsHolder<RedisPubSubConnection>
  │     │     │     └─ idleConnectionWatcher             ← 同 ServiceManager.connectionWatcher
  │     │     ├─ masterConnectionPool: MasterConnectionPool
  │     │     │     └─ (extends ConnectionPool<RedisConnection>)
  │     │     ├─ masterPubSubConnectionPool: MasterPubSubConnectionPool
  │     │     │     └─ (extends ConnectionPool<RedisPubSubConnection>)
  │     │     ├─ slaveConnectionPool: SlaveConnectionPool
  │     │     │     └─ (extends ConnectionPool<RedisConnection>)
  │     │     ├─ slavePubSubConnectionPool: PubSubConnectionPool
  │     │     │     └─ (extends ConnectionPool<RedisPubSubConnection>)
  │     │     └─ client2Entry: Map<RedisClient, ClientConnectionsEntry>
  │     │           └─ ClientConnectionsEntry            (每个 slave 节点一条)
  │     │                 ├─ client: RedisClient         (slave 节点)
  │     │                 ├─ nodeType: SLAVE
  │     │                 ├─ connectionsHolder: ConnectionsHolder<RedisConnection>
  │     │                 └─ pubSubConnectionsHolder: ConnectionsHolder<RedisPubSubConnection>
  │     │
  │     └─ subscribeService: PublishSubscribeService
  │           ├─ connectionManager                       ← 反向引用
  │           ├─ config: MasterSlaveServersConfig        ← 同上
  │           ├─ locks: AsyncSemaphore[50]
  │           ├─ name2entry: Map<ChannelName, Collection<PubSubConnectionEntry>>
  │           │     └─ PubSubConnectionEntry
  │           ├─ entry2PubSubConnection: Map<MasterSlaveEntry, PubSubEntry>
  │           │     └─ PubSubEntry
  │           │           └─ Queue<PubSubConnectionEntry>   ← 同上
  │           ├─ lockPubSub: LockPubSub
  │           ├─ semaphorePubSub: SemaphorePubSub
  │           └─ countDownLatchPubSub: CountDownLatchPubSub
  │
  ├─ commandExecutor: CommandAsyncService (implements CommandAsyncExecutor)
  │     ├─ connectionManager                             ← 同上
  │     ├─ codec: Codec                                  ← 同 Config.codec
  │     ├─ objectBuilder: RedissonObjectBuilder
  │     │     └─ referenceType: ReferenceType (enum: RxJava/Reactive/Default)
  │     ├─ retryAttempts: int
  │     ├─ retryDelay: DelayStrategy
  │     ├─ responseTimeout: int
  │     ├─ trackChanges: boolean
  │     │
  │     └─ [每次 writeAsync/readAsync 调用时临时创建] RedisExecutor<V, R>
  │           ├─ readOnlyMode: boolean
  │           ├─ command: RedisCommand<V>                (要执行的具体命令)
  │           ├─ params: Object[]                        (命令参数，编码前)
  │           ├─ source: NodeSource                      (目标节点来源：slot/addr/master等)
  │           ├─ codec: Codec                            ← 来自调用方
  │           ├─ connectionManager                       ← 同上
  │           ├─ objectBuilder: RedissonObjectBuilder    ← 同上
  │           ├─ referenceType: ReferenceType            ← 同上
  │           ├─ mainPromise: CompletableFuture<R>       (结果 promise)
  │           ├─ connectionFuture: CompletableFuture<RedisConnection>
  │           ├─ entry: MasterSlaveEntry                 (执行期间路由到的节点组)
  │           ├─ attempts / retryStrategy / responseTimeout  (重试/超时控制)
  │           ├─ attempt: int (volatile)                 (当前重试次数)
  │           ├─ timeout: Optional<Timeout> (volatile)   (当前超时句柄)
  │           ├─ writeFuture: ChannelFuture (volatile)   (Netty 写出 future)
  │           ├─ ignoreRedirect: boolean
  │           ├─ noRetry: boolean
  │           └─ trackChanges: boolean
  │
  ├─ evictionScheduler: EvictionScheduler
  │     ├─ executor: CommandAsyncExecutor                ← 同上
  │     └─ tasks: Map<String, EvictionTask>
  │           └─ EvictionTask (abstract, implements TimerTask)
  │                 ├─ executor: CommandAsyncExecutor    ← 同上
  │                 ├─ sizeHistory: Deque<Integer>
  │                 ├─ delay / minDelay / maxDelay: int
  │                 └─ keysLimit: int
  │
  └─ writeBehindService: WriteBehindService
        ├─ executor: CommandAsyncExecutor                ← 同上
        └─ tasks: Map<String, MapWriteBehindTask>


RedissonObject (abstract, implements RObject)       各 Redis 对象的公共基类
  ├─ commandExecutor: CommandAsyncExecutor          ← 同上
  ├─ name: String                                   (Redis key)
  ├─ codec: Codec                                   ← 同上，由 ServiceManager 规范化
  └─ listeners: Map<String, Collection<Integer>>
        (子类) RedissonBucket ──► RedissonObject
        (子类) RedissonLock   ──► RedissonBaseLock ──► RedissonExpirable ──► RedissonObject
        (子类) RedissonMap    ──► RedissonObject


RedissonKeys (implements RKeys)                     按需创建，不是 Redisson 的持久字段
  └─ commandExecutor: CommandAsyncExecutor          ← 同上
        (通过 commandExecutor 间接访问 ConnectionManager、ServiceManager 等)
        创建方式: redisson.getKeys() → new RedissonKeys(commandExecutor)
```

---

## 五、构造顺序（Redisson.create(config) 调用链）

```
1. Config（用户填写）
2. ConnectionManager.create(config)
   → 根据 Config 中的服务器模式，实例化对应 ConnectionManager
   → 内部构造 ServiceManager（持有 Netty 资源）
   → 内部构造 PublishSubscribeService
   → 内部构造 MasterSlaveEntry（建立真实 TCP 连接）
   → 若非 lazyInitialization，立即调用 connect()
3. commandExecutor = cm.createCommandExecutor(objectBuilder, referenceType)
   → 实例化 CommandAsyncService
4. evictionScheduler = new EvictionScheduler(commandExecutor)
5. writeBehindService = new WriteBehindService(commandExecutor)
6. new Redisson(上述所有对象)
```
