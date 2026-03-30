# redisson-rs Architecture Notes

## 关键架构差异

### 1. 继承改为组合

Java 使用继承链组织 Redisson 对象。Rust 中优先使用组合实现，例如 `RedissonLock` 持有 `RedissonBaseLock`，再通过 `Deref` 暴露共同行为。

### 2. 编解码改为类型约束

Java 依赖运行时 `Codec`。Rust 侧主要通过 `TryInto<Value>` 和 `FromValue` 组织写入与读取，尽量把序列化约束前移到类型系统。

### 3. 泛型能力与 dyn trait 分离

Java 接口可直接暴露泛型方法。Rust 中对象安全和泛型方法不能同时兼得，因此：

- trait 层保留 object-safe 能力，给动态分发路径使用。
- 具体实现层保留泛型方法，给实际业务调用使用。

### 4. 线程标识改为 task 标识

Java 锁实现依赖 `threadId`。Rust 中对应使用 `tokio::task::id()`，并沿用 `uuid:taskId` 这一整体标识格式。

## 当前实现范围

- [x] `RLock` / `RedissonLock`
- [x] watchdog 自动续约
- [x] Cluster Sharded Pub/Sub 自动探测
- [x] Standalone / Cluster / Sentinel 三种拓扑
- [x] `RBucket` 基础实现
- [x] `RBatch` 基础实现
- [ ] `RReadWriteLock`
- [ ] 公平锁
- [ ] MultiLock / RedLock
- [ ] Map、Queue、Semaphore 等其他 Redisson 对象

## 工作原则

- 先看 `redisson/` 对应实现，再决定 Rust 侧接口和行为。
- 优先保证行为对齐，其次才是局部写法是否“更 Rust”。
- 发现目录结构继续变化时，优先同步更新文档，避免说明和代码脱节。
