# Batch Executor 类关系与依赖图

对应 Java 包：`org.redisson.command` / `org.redisson.client.protocol`
Rust 对应路径：`src/command/` / `src/client/protocol/`

---

## 一、继承 / 组合层级（Java → Rust 对比）

```
Java（继承）                              Rust（组合）

RedisExecutor<V,R>                       RedisExecutor<V,R>
  ├── BaseRedisBatchExecutor<V,R>          BaseRedisBatchExecutor<V,R>
  │     ├── RedisBatchExecutor<V,R>          .inner: RedisExecutor<V,R>
  │     └── RedisQueuedBatchExecutor<V,R>
  └── RedisCommonBatchExecutor           RedisBatchExecutor<V,R>
        (固定泛型 <Object,Void>)            .base: BaseRedisBatchExecutor<V,R>

                                         RedisQueuedBatchExecutor<V,R>
                                           .base: BaseRedisBatchExecutor<V,R>
                                           .connections: Arc<Mutex<Vec<(Arc<MasterSlaveEntry>, ConnectionEntry)>>>
                                           .aggregated_commands: Arc<Mutex<Vec<(Arc<MasterSlaveEntry>, Entry)>>>

                                         RedisCommonBatchExecutor
                                           .inner: RedisExecutor<fred::Value, ()>
                                           .entry: Entry
                                           .slots: Arc<AtomicU32>
                                           .options: BatchOptions
```

---

## 二、各类职责与使用场景

| 类 | 执行模式 | 核心职责 |
|---|---|---|
| `RedisExecutor<V,R>` | 所有模式的基础 | 单条命令全生命周期：连接获取、发送、超时、重试、MOVED/ASK 重定向 |
| `BaseRedisBatchExecutor<V,R>` | batch 公共基类 | 在 RedisExecutor 上增加 `commands`（命令表）、`options`、`index`、`executed`；提供 `add_batch_command_data()` |
| `RedisBatchExecutor<V,R>` | `IN_MEMORY` / `IN_MEMORY_ATOMIC` | 覆写 `execute()`：调用 `add_batch_command_data()` 将命令入队，不直接发送 |
| `RedisQueuedBatchExecutor<V,R>` | `REDIS_WRITE_ATOMIC` / `REDIS_READ_ATOMIC` | 覆写 `execute()`、`send_command()`、`get_connection()` 等；每节点维护专用连接，首命令注入 MULTI，最后发 EXEC |
| `RedisCommonBatchExecutor` | `IN_MEMORY` / `IN_MEMORY_ATOMIC` | 每个节点对应一个实例，将该节点的整个 `Entry`（所有命令）打包为一次 pipeline 发送；`slots` 归零时完成 `mainPromise` |

---

## 三、核心数据结构关系

```
CommandBatchService
  │
  ├── commands: Vec<(NodeSource, Entry)>
  │                           │
  │                           └── Entry
  │                                 ├── commands: VecDeque<Box<dyn Any>>  ←─── BatchCommandData<V,R>
  │                                 ├── eval_commands: Vec<Box<dyn Any>>  ←─── BatchCommandData（仅 EVAL）
  │                                 └── read_only_mode: bool
  │
  ├── connections: Vec<(Arc<MasterSlaveEntry>, ConnectionEntry)>
  │                                              │
  │                                              └── ConnectionEntry
  │                                                    ├── first_command: bool
  │                                                    ├── connection_future: Arc<CompletableFuture<RedisConnection>>
  │                                                    └── cancel_callback: Option<Box<dyn FnOnce()>>
  │
  ├── aggregated_commands: Vec<(Arc<MasterSlaveEntry>, Entry)>
  │       （REDIS_*_ATOMIC 模式下按节点聚合，供 EXEC 时使用）
  │
  └── options: BatchOptions
        ├── execution_mode: ExecutionMode
        ├── retry_attempts / retry_delay
        ├── response_timeout / sync_timeout
        └── sync_slaves / sync_locals / sync_aof / skip_result
```

---

## 四、BatchCommandData 与周边类型关系

```
BatchCommandData<T,R>
  ├── inner: CommandData<T,R>
  │     ├── promise: Arc<CompletableFuture<R>>   ← 命令结果 Promise
  │     ├── command: RedisCommand<T>
  │     ├── params: Vec<fred::Value>
  │     └── codec: Option<Box<dyn Codec>>
  ├── index: u32                                 ← 全局序号（排序依据）
  └── retry_error: Mutex<Option<Box<dyn Error>>> ← 重试失败原因
```

`BatchCommandData` 实现了 `Ord`（按 `index` 升序），用于 batch 完成后对多节点结果做全局排序。

---

## 五、BatchPromise 与 CommandBatchService 的协作

```
BatchPromise<R>
  ├── promise: Arc<CompletableFuture<R>>        ← EXEC 返回后完成
  └── sent_promise: Arc<CompletableFuture<()>>  ← 命令写入连接后完成

CommandBatchService.execute_redis_based_queue()
  1. 收集所有命令的 sent_promise
  2. 等待 allOf(sent_promises) → 所有命令已入 MULTI 队列
  3. 发送 EXEC → 等待各节点的 BatchPromise 完成
  4. 聚合结果 → BatchResult
```

仅在 `REDIS_WRITE_ATOMIC` / `REDIS_READ_ATOMIC` 模式下使用 `BatchPromise`；
`IN_MEMORY` / `IN_MEMORY_ATOMIC` 模式下直接使用 `CompletableFuture<R>`。

---

## 六、完整依赖关系图（模块视角）

```
src/command/
  command_batch_service.rs
    ├── 持有 → Entry（内部定义）
    ├── 持有 → ConnectionEntry（内部定义）
    ├── 使用 → BatchOptions（src/api/batch_options.rs）
    ├── 使用 → BatchResult（src/api/batch_result.rs）
    ├── 使用 → BatchCommandData（src/client/protocol/batch_command_data.rs）
    ├── 使用 → NodeSource（src/connection/node_source.rs）
    ├── 使用 → MasterSlaveEntry（src/connection/master_slave_entry.rs）
    ├── 使用 → CompletableFuture（src/command/redis_executor.rs）
    └── 实现 → BatchService（src/command/batch_service.rs，marker trait）

  redis_executor.rs
    ├── 定义 → CompletableFuture<T>（占位）
    ├── 定义 → ChannelFuture（占位）
    ├── 定义 → Timeout（占位）
    ├── 使用 → NodeSource
    ├── 使用 → MasterSlaveEntry
    ├── 使用 → RedisConnection（src/client/redis_connection.rs）
    ├── 使用 → RedissonObjectBuilder（src/liveobject/core/）
    └── 使用 → DelayStrategy（src/config/equal_jitter_delay.rs）

  base_redis_batch_executor.rs
    ├── 组合 → RedisExecutor<V,R>（inner 字段）
    ├── 持有 → Arc<Mutex<Vec<(NodeSource, Entry)>>>（commands）
    ├── 持有 → BatchOptions
    ├── 持有 → Arc<AtomicU32>（index）
    └── 持有 → Arc<AtomicBool>（executed）

  redis_batch_executor.rs
    └── 组合 → BaseRedisBatchExecutor<V,R>（base 字段）

  redis_queued_batch_executor.rs
    ├── 组合 → BaseRedisBatchExecutor<V,R>（base 字段）
    ├── 持有 → Arc<Mutex<Vec<(Arc<MasterSlaveEntry>, ConnectionEntry)>>>（connections）
    └── 持有 → Arc<Mutex<Vec<(Arc<MasterSlaveEntry>, Entry)>>>（aggregated_commands）

  redis_common_batch_executor.rs
    ├── 组合 → RedisExecutor<fred::Value, ()>（inner 字段）
    ├── 持有 → Entry（当前节点命令组）
    ├── 持有 → Arc<AtomicU32>（slots，多节点计数器）
    └── 持有 → BatchOptions

  batch_promise.rs
    ├── 持有 → Arc<CompletableFuture<T>>（promise）
    └── 持有 → Arc<CompletableFuture<()>>（sent_promise）

src/client/protocol/
  batch_command_data.rs
    └── 组合 → CommandData<T,R>（inner 字段）

  command_data.rs
    ├── 持有 → Arc<CompletableFuture<R>>（promise）
    ├── 持有 → RedisCommand<T>（command）
    └── 持有 → Option<Box<dyn Codec>>（codec）
```

---

## 七、执行流程中的数据流向（IN_MEMORY 模式）

```
用户调用 RBatch.get_xxx().set(...)
    │
    ▼
CommandBatchService::async()
    │  创建 RedisBatchExecutor<V,R>
    │
    ▼
RedisBatchExecutor::execute()
    │  add_batch_command_data(params)
    │  → 按 NodeSource 查找 Entry，将 BatchCommandData 加入 Entry.commands
    │
    ▼
CommandBatchService::execute()
    │  按 Entry 遍历各节点
    │  为每节点创建 RedisCommonBatchExecutor
    │
    ▼
RedisCommonBatchExecutor::send_command()
    │  将 Entry.commands 打包为 CommandsData pipeline 发送
    │
    ▼
slots.decrementAndGet() == 0 时 → mainPromise 完成 → BatchResult 返回
```

---

## 八、执行流程中的数据流向（REDIS_WRITE_ATOMIC 模式）

```
用户调用 RBatch.get_xxx().set(...)
    │
    ▼
CommandBatchService::async()
    │  创建 RedisQueuedBatchExecutor<V,R>（mainPromise 为 BatchPromise）
    │
    ▼
RedisQueuedBatchExecutor::execute()
    │  命令加入 aggregated_commands[MasterSlaveEntry]
    │
    ▼
RedisQueuedBatchExecutor::get_connection()
    │  按节点 computeIfAbsent → ConnectionEntry（first_command=true）
    │
    ▼
RedisQueuedBatchExecutor::send_command()
    │  first_command=true  → 发 [MULTI, 命令]，置 first_command=false
    │  命令入队后           → sent_promise 完成
    │  发 EXEC 时          → 带上 Entry.commands 作为 CommandsData 的预期响应列表
    │
    ▼
CommandBatchService::execute_redis_based_queue()
    │  allOf(all sent_promises) → 所有节点命令已入队
    │  等待各节点 BatchPromise 完成（EXEC 响应）
    │
    ▼
聚合 aggregated_commands 中的 BatchCommandData 结果 → 按 index 排序 → BatchResult 返回
```
