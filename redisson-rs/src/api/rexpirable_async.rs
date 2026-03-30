use crate::api::robject_async::RObjectAsync;
use anyhow::Result;
use std::future::Future;
use std::time::Duration;

// ============================================================
// RExpirableAsync — 对应 Java org.redisson.api.RExpirableAsync（接口）
// ============================================================

/// Base async interface for all Redisson objects which support expiration or TTL
/// 对应 Java RExpirableAsync extends RObjectAsync
pub trait RExpirableAsync: RObjectAsync {
    // ── expire 系列（设置过期时间）────────────────────────────────────

    /// Set a timeout for object. After the timeout has expired,
    /// the key will automatically be deleted.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireAsync(Duration duration)
    fn expire(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send;

    /// Set an expire date for object. When expire date comes
    /// the key will automatically be deleted.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireAsync(Instant time)
    fn expire_at(&self, timestamp_millis: u64) -> impl Future<Output = Result<bool>> + Send;

    /// Set an expire date for object only if it has been already set.
    /// Requires Redis 7.0.0 and higher.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireIfSetAsync(Duration duration)
    fn expire_if_set(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send;

    /// Set an expire date for object only if it hasn't been set before.
    /// Requires Redis 7.0.0 and higher.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireIfNotSetAsync(Duration duration)
    fn expire_if_not_set(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send;

    /// Set an expire date for object only if it's greater than expire date set before.
    /// Requires Redis 7.0.0 and higher.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireIfGreaterAsync(Duration duration)
    fn expire_if_greater(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send;

    /// Set an expire date for object only if it's less than expire date set before.
    /// Requires Redis 7.0.0 and higher.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> expireIfLessAsync(Duration duration)
    fn expire_if_less(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send;

    // ── clearExpire（清除过期）────────────────────────────────────────

    /// Clear an expire timeout or expire date for object.
    /// Object will not be deleted.
    /// 对应 Java RExpirableAsync: RFuture<Boolean> clearExpireAsync()
    fn clear_expire(&self) -> impl Future<Output = Result<bool>> + Send;

    // ── 查询过期信息 ────────────────────────────────────────────────

    /// Returns remaining time of the object in milliseconds.
    /// Returns -2 if the key does not exist, -1 if the key exists but has no associated expire.
    /// 对应 Java RExpirableAsync: RFuture<Long> remainTimeToLiveAsync()
    fn remain_time_to_live(&self) -> impl Future<Output = Result<i64>> + Send;

    /// Returns expiration time of the object as the absolute Unix expiration timestamp in milliseconds.
    /// Requires Redis 7.0.0 and higher.
    /// 对应 Java RExpirableAsync: RFuture<Long> getExpireTimeAsync()
    fn get_expire_time(&self) -> impl Future<Output = Result<i64>> + Send;
}
