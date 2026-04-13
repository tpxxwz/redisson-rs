# 命令执行内部机制分析：NodeSource / RedisClient / RedisExecutor

Java `CommandAsyncService` 的命令执行链路涉及三个核心角色：
- **NodeSource**：携带路由决策所需的节点信息
- **RedisClient**：底层 TCP 连接管理器
- **RedisExecutor**：单次命令执行的生命周期管理器（重试 / 超时 / 重定向 / 结果解码）

---

## NodeSource — 路由信息容器

位置：`org.redisson.connection.NodeSource`

### 字段

```java
public class NodeSource {
    private Integer slot;           // 哈希槽（Cluster 模式分片键）
    private RedisURI addr;          // 重定向目标地址（MOVED / ASK 时使用）
    private RedisClient redisClient;// 指定特定客户端（精确路由到某个从节点）
    private Redirect redirect;      // 重定向类型：MOVED | ASK | REDIRECT
    private MasterSlaveEntry entry; // 主从连接池容器（已知 entry 时直接传入）
}
```

### 五种构造场景

| 构造方式 | 含义 | 典型场景 |
|---------|------|---------|
| `new NodeSource(slot)` | 只有槽号，由路由逻辑推导节点 | `readAsync(String key, ...)` / `writeAsync(String key, ...)` |
| `new NodeSource(entry)` | 直接指定 entry | `writeAsync(MasterSlaveEntry entry, ...)` |
| `new NodeSource(entry, client)` | 指定 entry + 特定客户端（从节点读） | 读写分离场景 |
| `new NodeSource(client)` | 只指定客户端 | `readAsync(RedisClient client, ...)` |
| `new NodeSource(slot, addr, redirect)` | 重定向：槽号 + 新地址 + 类型 | 处理 MOVED / ASK 集群错误时重试 |

### 路由决策优先级（RedisExecutor.getEntry()）

```
1. redirect != null    → connectionManager.getEntry(source.getAddr())     // 重定向地址
2. entry != null       → 直接使用 source.getEntry()
3. redisClient != null → connectionManager.getEntry(source.getRedisClient())
4. slot != null        → read ? getReadEntry(slot) : getWriteEntry(slot)  // 按槽找节点
```

---

## RedisClient — 底层 TCP 连接管理器

位置：`org.redisson.client.RedisClient`

### 职责

- 封装 Netty Bootstrap，管理与**单个 Redis 节点**的 TCP 连接
- 提供 `connectAsync()` → `RedisConnection`（命令连接）
- 提供 `connectPubSubAsync()` → `RedisPubSubConnection`（发布/订阅连接）
- 管理 DNS 解析、TLS、多种网络传输（NIO / Epoll / KQueue / io_uring）

### 在命令路由中的位置

```
NodeSource.redisClient
    ↓ (如果指定了特定客户端)
MasterSlaveEntry.getEntry(redisClient)
    ↓
slaveConnectionPool.get(command, entry)  // 从指定从节点的连接池取连接
    ↓
RedisClient.connectAsync()              // 建立/复用 TCP 连接
```

RedisClient 本身不执行命令，只负责建连和管道管理。
实际命令发送通过 `RedisConnection.async(Command, params)` 完成。

---

## MasterSlaveEntry — 主从连接池容器

一个 entry 代表**一对主从节点**，内部维护四个连接池：

| 连接池 | 用途 |
|-------|------|
| `masterConnectionPool` | 主节点命令连接 |
| `masterPubSubConnectionPool` | 主节点 Pub/Sub 连接 |
| `slaveConnectionPool` | 从节点命令连接（ReadMode 控制是否启用） |
| `slavePubSubConnectionPool` | 从节点 Pub/Sub 连接 |

读操作根据 `ReadMode` 配置决定走主还是从：

```java
if (config.getReadMode() == ReadMode.MASTER) {
    return connectionWriteOp(command);   // 只读主
}
return slaveConnectionPool.get(...);    // 读从
```

---

## RedisExecutor — 单次命令执行的生命周期管理器

位置：`org.redisson.command.RedisExecutor`

`CommandAsyncService.async()` 每次调用都会 `new RedisExecutor(...)` 并调用 `executor.execute()`。
RedisExecutor 负责从"拿到 NodeSource"到"Promise 最终完成"之间的全部细节。

### 关键字段

| 字段 | 类型 | 作用 |
|-----|------|------|
| `source` | `NodeSource` | 目标节点路由信息 |
| `readOnlyMode` | boolean | 是否走从节点读 |
| `command` | `RedisCommand<V>` | 要执行的命令 |
| `params` | `Object[]` | 命令参数 |
| `mainPromise` | `CompletableFuture<R>` | 最终结果（调用方等待此 Promise） |
| `attempts` | int | 最大重试次数（来自 `retryAttempts` 配置） |
| `retryStrategy` | `DelayStrategy` | 重试延迟计算策略 |
| `responseTimeout` | int | 响应超时（毫秒） |
| `ignoreRedirect` | boolean | 是否忽略 MOVED/ASK 重定向（batch 模式用） |
| `noRetry` | boolean | 强制禁用重试 |
| `attempt` | volatile int | 当前已重试次数 |
| `objectBuilder` | `RedissonObjectBuilder` | 结果解码时的 Live Object 转换器 |

### execute() 主流程

```
execute()
  ├─ 1. 检查 mainPromise 是否已取消 / 系统是否关闭
  ├─ 2. 获取连接（connectionReadOp 或 connectionWriteOp）
  │      └─ 调用 getEntry() 从 NodeSource 提取路由信息 → MasterSlaveEntry
  ├─ 3. 启动三个并发超时任务：
  │      ├─ scheduleRetryTimeout()       — 重试间隔超时
  │      ├─ scheduleConnectionTimeout()  — 连接获取超时
  │      └─ scheduleWriteTimeout()       — 命令写入超时（连接成功后启动）
  ├─ 4. 连接就绪后发送命令（sendCommand）
  │      └─ ASK 重定向时先发 ASKING 再发命令
  ├─ 5. 命令写入成功后启动 scheduleResponseTimeout()
  └─ 6. attemptPromise 完成后 checkAttemptPromise()
         ├─ 成功 → handleSuccess()
         ├─ 重定向（MOVED/ASK） → handleRedirect() → 递归 execute()
         └─ 可重试错误 → 延迟后递增 attempt，递归 execute()
```

### 重试逻辑

**三个重试触发入口**：

1. **scheduleRetryTimeout()**：`retryInterval > 0` 时，按间隔定时检查是否需要重试
2. **checkAttemptPromise()**：命令完成后，根据异常类型决定是否重试
3. **scheduleResponseTimeout()**：响应超时后，通过 `isResendAllowed()` 判断是否重试

**触发重试的错误类型**：

| 错误 | 触发原因 |
|-----|---------|
| `RedisWrongPasswordException` | 认证失败 |
| `RedisRetryException` | 服务端指示可重试 |
| `RedisReadonlyException` | 主从切换中 |
| `RedisReconnectedException` | 连接断开可重发 |
| `RedisLoadingException` | 从节点正在加载数据 |
| 响应超时 | 命令未在 responseTimeout 内返回 |

重试上限：`attempt < attempts`（到达上限后 `handleError()` 失败）。

### MOVED / ASK 重定向处理

`checkAttemptPromise()` 捕获到 `RedisRedirectException` 且 `ignoreRedirect == false` 时：

```java
// 防止重定向循环
if (source.getRedirect() == MOVED && source.getAddr().equals(ex.getUrl())) {
    mainPromise.completeExceptionally(...);
    return;
}

// 根据异常类型区分 MOVED / ASK
Redirect reason = (cause instanceof RedisMovedException) ? MOVED : ASK;
handleRedirect(ex, connectionFuture, reason);
```

`handleRedirect()` 做的事：
1. 解析重定向目标 IP（DNS）
2. 构造新的 `NodeSource(slot, newAddr, reason)`
3. 递归调用 `execute()`

**ASK 特殊处理**：`sendCommand()` 中检测到 `source.getRedirect() == ASK` 时，
先发 `ASKING` 命令，再发实际命令（Redis Cluster 协议规范要求）。

**ignoreRedirect**：
- `true` 时收到 MOVED/ASK 也不重定向，直接以错误返回
- 用于 batch 模式：防止命令跟随重定向分裂到不同节点，破坏 batch 原子性

### 超时机制

| 超时类型 | 启动时机 | 超时时间 | 行为 |
|---------|---------|---------|------|
| 连接获取超时 | execute() 开始 | `responseTimeout` | 抛 `RedisTimeoutException` |
| 写入超时 | execute() 开始 | `responseTimeout` | 尝试取消写入 |
| 响应超时 | 命令写入成功后 | `responseTimeout`（阻塞命令额外加命令自身 timeout + 1000ms） | 尝试重试或失败 |

### handleSuccess() — 结果解码与 objectBuilder

```java
if (objectBuilder != null) {
    // 将结果中的 RedissonReference 还原为 Live Object 实例
    promise.complete((R) objectBuilder.tryHandleReference(res, referenceType));
} else {
    promise.complete(res);
}
// 通知故障检测器该节点命令成功
connectionFuture.join().getRedisClient()
    .getConfig().getFailedNodeDetector().onCommandSuccessful();
```

### handleError() — 失败处理与故障检测

```java
mainPromise.completeExceptionally(cause);

FailedNodeDetector detector = client.getConfig().getFailedNodeDetector();
detector.onCommandFailed(cause);

if (detector.isNodeFailed()) {
    entry.shutdownAndReconnectAsync(client, cause);  // 标记节点故障，触发重连
}
```

---

## 完整命令执行链路（Java）

```
CommandAsyncExecutor.readAsync(key, codec, command, params)
  → calcSlot(key) → new NodeSource(slot)
  → CommandAsyncService.async(readOnly=true, source, ...)
  → new RedisExecutor(source, retryAttempts, retryDelay, responseTimeout, ...)
  → executor.execute()
      → getEntry(read=true)              // NodeSource 路由决策 → MasterSlaveEntry
      → entry.connectionReadOp(cmd)      // 连接池借连接（主或从节点）
      → scheduleRetryTimeout / scheduleConnectionTimeout / scheduleWriteTimeout
      → connection.async(cmd, params)    // Netty 发送命令
      → scheduleResponseTimeout
      → checkAttemptPromise()
          ├─ 成功 → handleSuccess() → objectBuilder.tryHandleReference → mainPromise.complete
          ├─ MOVED/ASK → handleRedirect() → 递归 execute()
          └─ 可重试 → 延迟后递归 execute()
      → releaseConnection                // 归还连接
```

---

## Rust / fred 侧的对应

### NodeSource → ClusterHash

fred 中 `CustomCommand::new_static(name, slot, blocking)` 的第二个参数 `ClusterHash`
承担了 NodeSource 最常见用途：**告诉 fred 把这条命令路由到哪个 slot 对应的节点**。

```rust
let slot = ClusterHash::Custom(self.connection_manager.calc_slot(key.into().as_bytes()));
let cmd = CustomCommand::new_static(command.name, slot, false);
pool.custom(cmd, all_args).await   // fred 内部根据 ClusterHash 完成路由
```

### NodeSource 其他场景在 fred 的对应

| Java NodeSource 场景 | fred 等价操作 |
|---------------------|-------------|
| `slot` → master | `pool.custom(cmd, args)` (默认走 master) |
| `slot` → replica（ReadMode） | `pool.replicas().custom(cmd, args)` |
| `entry` + 精确节点 | `entry.client_for_node().custom(cmd, args)` (通过 MasterSlaveEntry) |
| `redisClient` → 特定从节点 | `pool.next().with_cluster_node(&server)` |
| MOVED / ASK 重定向 | fred 集群模式**自动处理**，Rust 侧无需介入 |

### RedisExecutor → fred 透明管理

Java `RedisExecutor` 承担的职责，在 fred 中已内置，Rust 侧调用方只需 `.custom(...).await`：

| Java RedisExecutor 职责 | fred 对应 |
|------------------------|---------|
| 连接借还（`connectionReadOp` / `releaseConnection`） | Pool 内部连接池自动管理 |
| 重试（`attempt < attempts`） | fred `ReconnectPolicy` + 内部重试逻辑 |
| MOVED / ASK 重定向 | fred Cluster 模式自动跟随 |
| 响应超时 | fred 连接配置的 `response_timeout` |
| 故障检测（`onCommandFailed` / `shutdownAndReconnectAsync`） | fred `ReconnectPolicy` 自动重连 |
| `handleSuccess` 中 `objectBuilder` 调用 | 暂未实现（见 `redisson-object-builder-todo.md`） |

---

## 待实现 / 差距

1. **精确节点操作（executeAll / SCAN 全节点）**
   - Java：`getEntrySet()` → for entry → `entry.connectionWriteOp` → `connection.async`
   - Rust：`get_entry_set()` → for entry → `entry.client_for_node().custom(cmd, args).await`
   - 状态：`client_for_node()` 已实现，上层调用方尚未实现

2. **Cluster 模式 getWriteEntry(slot)**
   - 当前 `FredConnectionManager::get_write_entry` 总是取第一个活跃节点
   - Cluster 下应通过 fred cluster state 查找 slot 对应 master
   - 状态：TODO，见 `fred_connection_manager.rs` 注释

3. **读写分离（ReadMode.SLAVE）**
   - `use_replica_for_reads=true` 时 `exec()` 已走 `pool.replicas()`
   - 精确指定从节点（NodeSource.redisClient 场景）暂未实现
   - 状态：当前降级为 `pool.replicas()` 整体副本集

4. **ignoreRedirect（Batch 模式）**
   - Java `ignoreRedirect` 防止 batch 内命令跟随 MOVED/ASK 分裂
   - fred Pool 没有暴露此选项，batch 模式下存在潜在风险
   - 状态：TODO，见 `command_async_service.rs` 注释

5. **handleSuccess 的 objectBuilder 调用**
   - Java 在每条命令成功后通过 `objectBuilder.tryHandleReference()` 还原 Live Object 引用
   - Rust 侧 `CommandAsyncService::exec()` 当前只调用 `convertor`，无 objectBuilder 介入
   - 状态：待实现，见 `redisson-object-builder-todo.md`
