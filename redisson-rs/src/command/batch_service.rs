// ============================================================
// BatchService — 对应 Java org.redisson.command.BatchService（接口）
// ============================================================

/// 对应 Java org.redisson.command.BatchService。
/// Java 中为空接口（marker interface），用于标记批处理服务类型。
/// Rust 侧同样作为 marker trait 使用。
pub trait BatchService: Send + Sync {}
