use crate::api::rexpirable_async::RExpirableAsync;

// ============================================================
// RExpirable — 对应 Java org.redisson.api.RExpirable（接口）
// ============================================================

/// Base interface for all Redisson objects which support expiration or TTL
/// 对应 Java RExpirable extends RExpirableAsync, RObject
///
/// 在 Rust 中，RExpirable 继承 RExpirableAsync 的所有方法，
/// 因为 Rust 的 async 方法天然就是异步的。
pub trait RExpirable: RExpirableAsync {}
