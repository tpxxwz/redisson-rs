# RedissonObjectBuilder — 分析结论与 Rust 侧待办

## 核心职责

`RedissonObjectBuilder` 控制的是 **RObject ↔ RedissonReference 的双向转换**，是"把一个 Redis 对象存进另一个 Redis 对象"这个能力的基础设施。

Java 中通过构造函数传入：

```java
RedissonObjectBuilder objectBuilder = config.isReferenceEnabled()
    ? new RedissonObjectBuilder(this)
    : null;
commandExecutor = connectionManager.createCommandExecutor(objectBuilder, ReferenceType.DEFAULT);
```

---

## 三大核心功能

### 1. 编码阶段（写入 Redis 前）

`CommandAsyncService.encode(codec, value)` 中检测值是否为 RObject，若是则转为引用：

```java
if (isRedissonReferenceSupportEnabled()) {
    RedissonReference ref = objectBuilder.toReference(value);
    if (ref != null) value = ref;  // 用引用替换原值再编码
}
```

例：`map.put("key", anotherRMap)` → 存入的是 `RedissonReference{type="RMap", keyName="..."}` 而非 `anotherRMap` 本身。

### 2. 解码阶段（从 Redis 收到后）

`RedisExecutor.handleSuccess()` 中递归还原引用：

```java
objectBuilder.tryHandleReference(result, referenceType)
```

把 Redis 返回的 `RedissonReference` 还原为真正的 `RMap`/`RList` 等实例（通过反射调用 `redisson.getMap(keyName)`）。

支持嵌套结构：List、Set、Map、ScanResult 内的引用都会被递归处理。

### 3. ReferenceType 控制返回 API 风格

| ReferenceType | 返回类型 | 对应客户端 |
|---|---|---|
| `DEFAULT` | `RObject`（同步） | `RedissonClient` |
| `REACTIVE` | `RObjectReactive` | `RedissonReactiveClient` |
| `RXJAVA` | `RObjectRx` | `RedissonRxClient` |

---

## 何时 objectBuilder 为 null / 不生效

- `config.isReferenceEnabled() == false` 时 objectBuilder 为 null，所有引用功能跳过
- 普通的 set/get/list/map 操作完全不走 objectBuilder 逻辑
- `isRedissonReferenceSupportEnabled()` 检查：`return objectBuilder != null`

---

## Rust 侧现状

`redisson-rs/src/liveobject/core/redisson_object_builder.rs` 当前是空占位结构体：

```rust
pub struct RedissonObjectBuilder;

impl Default for RedissonObjectBuilder {
    fn default() -> Self { RedissonObjectBuilder }
}

pub enum ReferenceType { RxJava, Reactive, Default }
```

`command_async_executor::create()` 工厂函数中直接忽略这两个参数：

```rust
pub fn create(
    connection_manager: Arc<dyn ConnectionManager>,
    object_builder: RedissonObjectBuilder,
    reference_type: ReferenceType,
) -> Arc<dyn CommandAsyncExecutor> {
    let _ = (object_builder, reference_type); // 待实现时填充
    Arc::new(CommandAsyncService::new(connection_manager))
}
```

这是完全合理的占位，不影响当前已实现的所有功能。

---

## 待实现（TODO）

以下两个场景需要真正实现 `RedissonObjectBuilder`：

### TODO 1：嵌套 RObject 引用

**场景**：把一个 `RMap`/`RList` 等 RObject 存入另一个 RMap 的 value。

**需要实现**：
- `RedissonObjectBuilder::to_reference(value)` — 检测值是否为 RObject，转为 `RedissonReference`
- `RedissonObjectBuilder::from_reference(reference, ref_type)` — 将 `RedissonReference` 还原为 RObject
- `CommandAsyncService::encode()` / `decode()` 中接入 objectBuilder 调用
- Rust 侧 `RedissonReference` 结构体（对标 `org.redisson.misc.RedissonReference`）

### TODO 2：Live Object（`@REntity` 注解对象持久化）

**场景**：通过 `getLiveObjectService()` 将普通 Rust struct 的字段自动映射到 Redis Hash。

**需要实现**：
- `redisson-rs/src/liveobject/` 目录下的核心框架（目前占位）
- `NamingScheme` — 实体 ID → Redis 键名映射
- `RedissonObjectBuilder::create_object()` — 为 Live Object 字段创建子 RObject
- `ReferenceType` 的实际分发逻辑（目前只有 `Default`）
