# redisson-rs

- [AI 助手] 始终用中文回复，禁止使用韩语或其他任何语言。

## 仓库关系

- 根目录 `redisson-rs/` 是总仓库。
- `redisson-rs/` 是当前 Rust 主开发目录。
- `fred/` 是第三方 Git submodule，上游为 `git@github.com:aembke/fred.rs.git`，当前固定在 `v10.1.0`。
- `redisson/` 是第三方 Git submodule，上游为 `git@github.com:redisson/redisson.git`，当前固定在 `redisson-4.3.0`。
- `redisson-rs/` 的实现目标是对标 `redisson/redisson/`，并基于 `fred/` 提供 Redis 能力。
- 对照 Java 实现时，只需要重点看 `redisson/redisson/` 和 `redisson/pom.xml`，`redisson/` 仓库里的其他目录和文件默认不需要关注。

## 当前目录

状态标记：

- `已实现`：当前目录已有较完整 Rust 实现并已接入主流程。
- `部分实现`：当前目录已有实现，但仍在持续补齐。
- `占位`：目录已建立，主要用于对齐结构，内容仍很少。
- `未开始`：Java 包存在，但 Rust 侧尚未建立对应目录或实现。

| Rust 路径 | Java 对应包 | 状态 | 说明 |
|---|---|---|---|
| `redisson-rs/src/api/` | `org.redisson.api` | 部分实现 | 对外暴露的顶层接口，如 `RLock`、`RMap`、`RBucket` 等所有 Redis 数据结构的抽象接口定义 |
| `-` | `org.redisson.cache` | 未开始 | 本地缓存实现，配合 `RLocalCachedMap` 使用，负责本地内存缓存策略（LRU、LFU 等） |
| `redisson-rs/src/client/` | `org.redisson.client` | 部分实现 | 底层 Redis 协议客户端，负责与单个 Redis 节点建立连接、发送 RESP 协议命令、处理响应 |
| `-` | `org.redisson.cluster` | 未开始 | Redis Cluster 拓扑管理，负责槽位映射、节点发现与故障转移 |
| `-` | `org.redisson.codec` | 未开始 | 序列化/反序列化编解码器，支持 JSON、Kryo、MsgPack 等多种格式 |
| `redisson-rs/src/command/` | `org.redisson.command` | 部分实现 | 命令执行器，负责将 API 调用路由到正确的节点，处理重试、读写分离等逻辑 |
| `redisson-rs/src/config/` | `org.redisson.config` | 部分实现 | 配置体系，涵盖单机、哨兵、集群、主从等各种部署模式的连接参数配置 |
| `redisson-rs/src/connection/` | `org.redisson.connection` | 部分实现 | 连接池管理，负责连接的创建、复用、心跳检测及多节点连接管理 |
| `-` | `org.redisson.eviction` | 未开始 | 过期/淘汰任务调度，用于 `RMapCache` 等带 TTL 的结构定期清理过期条目 |
| `-` | `org.redisson.executor` | 未开始 | 分布式任务执行框架（`RExecutorService`），支持将任务提交到 Redis 队列并在集群节点上执行 |
| `-` | `org.redisson.iterator` | 未开始 | 分布式集合的迭代器实现，基于 Redis `SCAN` 命令实现大集合的安全遍历 |
| `-` | `org.redisson.jcache` | 未开始 | JSR-107（JCache）规范适配层，使 Redisson 可作为标准 JCache 提供者使用 |
| `redisson-rs/src/liveobject/` | `org.redisson.liveobject` | 占位 | Live Object 框架，通过动态代理将普通 Java 对象的字段自动映射并持久化到 Redis Hash |
| `-` | `org.redisson.mapreduce` | 未开始 | 基于 Redis 的分布式 MapReduce 计算框架，用于在集群上并行处理大规模数据 |
| `redisson-rs/src/misc/` | `org.redisson.misc` | 占位 | 内部通用工具类，如 Promise、CompletableFuture 适配、Redis URI 解析等 |
| `redisson-rs/src/pubsub/` | `org.redisson.pubsub` | 部分实现 | 发布/订阅管理，复用连接处理 `SUBSCRIBE`/`PSUBSCRIBE`，为锁、信号量等上层机制提供通知基础 |
| `-` | `org.redisson.reactive` | 未开始 | Reactive Streams（Project Reactor）风格的异步 API 封装层 |
| `-` | `org.redisson.redisnode` | 未开始 | Redis 节点管理接口，用于查询节点信息、执行运维命令（如 `FLUSHDB`、`INFO`） |
| `-` | `org.redisson.remote` | 未开始 | 分布式远程服务（`RRemoteService`），基于 Redis 队列实现跨进程 RPC 调用 |
| `redisson-rs/src/renewal/` | `org.redisson.renewal` | 部分实现 | 锁续期（看门狗）机制，持有锁期间定期自动延长 TTL，防止业务未完成时锁过期 |
| `-` | `org.redisson.rx` | 未开始 | RxJava 风格的异步 API 封装层（与 reactive 包类似，针对 RxJava 2/3） |
| `-` | `org.redisson.transaction` | 未开始 | 分布式事务支持，通过 Redis 的 `MULTI/EXEC` 或 Lua 脚本实现原子性操作 |
| `redisson-rs/src/redisson.rs` 等根级文件 | `org.redisson` 根包下的核心类型 | 部分实现 | 根包核心类，包括 `Redisson`（客户端入口）、`RedissonObject`（所有对象基类）等 |

## 实现约定

Rust 侧新增文件或类型时，遵循以下约定：

1. 如果 Rust 类型明确对标 Java 中的类或接口，则 Rust 类型名尽量与 Java 类型名保持一致。
2. `redisson-rs/src/` 可视为 Java 的 `org.redisson` 包根路径。
3. 如果 Java 类型路径是 `org.redisson.xxx.yyy.Zzz`，则 Rust 文件优先映射到 `redisson-rs/src/xxx/yyy/zzz.rs`。
4. 例如 Java 的 `org.redisson.api.lock.RLock`，对应 Rust 文件应放在 `redisson-rs/src/api/lock/rlock.rs`，文件内类型名为 `RLock`。
5. Java 中的驼峰命名迁移到 Rust 时，尽量改成符合 Rust 习惯的命名形式；但如果 Rust 规范本来就要求使用驼峰，例如 `struct`、`trait`、`enum` 的类型名，则保持该写法。
6. 如果某个 Rust 文件是在对齐某个 Java 类，则字段顺序和方法顺序尽量与 Java 文件保持接近，方便逐段对照。
7. 如果某个辅助类型没有明确的 Java 对应物，优先放在当前实现附近，避免为了拆分而过早制造新层级。
8. 空实现或占位实现可以接受，但命名、目录归属和 Java 对齐关系必须先放对。
