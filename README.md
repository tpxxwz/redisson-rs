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

| Rust 路径 | Java 对应包 | 状态 |
|---|---|---|
| `redisson-rs/src/api/` | `org.redisson.api` | 部分实现 |
| `-` | `org.redisson.cache` | 未开始 |
| `redisson-rs/src/client/` | `org.redisson.client` | 部分实现 |
| `-` | `org.redisson.cluster` | 未开始 |
| `-` | `org.redisson.codec` | 未开始 |
| `redisson-rs/src/command/` | `org.redisson.command` | 部分实现 |
| `redisson-rs/src/config/` | `org.redisson.config` | 部分实现 |
| `redisson-rs/src/connection/` | `org.redisson.connection` | 部分实现 |
| `-` | `org.redisson.eviction` | 未开始 |
| `-` | `org.redisson.executor` | 未开始 |
| `-` | `org.redisson.iterator` | 未开始 |
| `-` | `org.redisson.jcache` | 未开始 |
| `redisson-rs/src/liveobject/` | `org.redisson.liveobject` | 占位 |
| `-` | `org.redisson.mapreduce` | 未开始 |
| `redisson-rs/src/misc/` | `org.redisson.misc` | 占位 |
| `redisson-rs/src/pubsub/` | `org.redisson.pubsub` | 部分实现 |
| `-` | `org.redisson.reactive` | 未开始 |
| `-` | `org.redisson.redisnode` | 未开始 |
| `-` | `org.redisson.remote` | 未开始 |
| `redisson-rs/src/renewal/` | `org.redisson.renewal` | 部分实现 |
| `-` | `org.redisson.rx` | 未开始 |
| `-` | `org.redisson.transaction` | 未开始 |
| `redisson-rs/src/redisson.rs` 等根级文件 | `org.redisson` 根包下的核心类型 | 部分实现 |

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
