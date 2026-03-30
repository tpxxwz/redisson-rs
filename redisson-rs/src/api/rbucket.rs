use anyhow::Result;
use std::future::Future;

// ============================================================
// RBucket — 对应 Java org.redisson.api.RBucketAsync
// ============================================================

/// 对应 Java RBucket<String>，Redis String 结构的异步操作接口。
pub trait RBucket {
    /// 对应 Java RBucketAsync.getAsync() — GET
    fn get(&self) -> impl Future<Output = Result<Option<String>>> + Send;

    /// 对应 Java RBucketAsync.setAsync(value) — SET
    fn set(&self, value: &str) -> impl Future<Output = Result<()>> + Send;

    /// 对应 Java RObjectAsync.deleteAsync() — DEL，返回是否删除成功
    fn delete(&self) -> impl Future<Output = Result<bool>> + Send;

    /// 对应 Java RBucketAsync.sizeAsync() — STRLEN
    fn size(&self) -> impl Future<Output = Result<i64>> + Send;
}
