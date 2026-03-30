use super::object_encoding::ObjectEncoding;
use super::object_listener::ObjectListener;
use anyhow::Result;
use bytes::Bytes;
use std::future::Future;
use std::time::Duration;

// ============================================================
// RObjectAsync — 对应 Java org.redisson.api.RObjectAsync（接口）
// ============================================================

/// Base asynchronous interface for all Redisson objects
/// 对应 Java RObjectAsync
///
/// 方法顺序与 Java RObjectAsync 保持一致
pub trait RObjectAsync: Send + Sync {
    // 0. getName
    /// Returns name of object.
    ///
    /// 对应 Java: String getName()
    /// 注：Java 通过 NameMapper.unmap 处理名称映射；Rust 版本目前直接返回 raw name。
    fn get_name(&self) -> String;

    // 1. getIdleTimeAsync
    /// Returns number of seconds spent since last write or read operation over this object.
    ///
    /// 对应 Java: RFuture<Long> getIdleTimeAsync()
    fn get_idle_time_async(&self) -> impl Future<Output = Result<i64>> + Send;

    // 2. getReferenceCountAsync
    /// Returns count of references over this object.
    ///
    /// 对应 Java: RFuture<Integer> getReferenceCountAsync()
    fn get_reference_count_async(&self) -> impl Future<Output = Result<i32>> + Send;

    // 3. getAccessFrequencyAsync
    /// Returns the logarithmic access frequency counter over this object.
    ///
    /// 对应 Java: RFuture<Integer> getAccessFrequencyAsync()
    fn get_access_frequency_async(&self) -> impl Future<Output = Result<i32>> + Send;

    // 4. getInternalEncodingAsync
    /// Returns the internal encoding for the Redis object
    ///
    /// 对应 Java: RFuture<ObjectEncoding> getInternalEncodingAsync()
    fn get_internal_encoding_async(&self) -> impl Future<Output = Result<ObjectEncoding>> + Send;

    // 5. sizeInMemoryAsync
    /// Returns bytes amount used by object in Redis memory.
    ///
    /// 对应 Java: RFuture<Long> sizeInMemoryAsync()
    fn size_in_memory_async(&self) -> impl Future<Output = Result<i64>> + Send;

    // 6. restoreAsync(byte[] state)
    /// Restores object using its state returned by dump() method.
    ///
    /// 对应 Java: RFuture<Void> restoreAsync(byte[] state)
    fn restore_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send;

    // 7. restoreAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    /// Restores object using its state returned by dump() method and set time to live for it.
    ///
    /// 对应 Java: RFuture<Void> restoreAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send;

    // 8. restoreAndReplaceAsync(byte[] state)
    /// Restores and replaces object if it already exists.
    ///
    /// 对应 Java: RFuture<Void> restoreAndReplaceAsync(byte[] state)
    fn restore_and_replace_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send;

    // 9. restoreAndReplaceAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    /// Restores and replaces object if it already exists and set time to live for it.
    ///
    /// 对应 Java: RFuture<Void> restoreAndReplaceAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_and_replace_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send;

    // 10. dumpAsync
    /// Returns dump of object
    ///
    /// 对应 Java: RFuture<byte[]> dumpAsync()
    fn dump_async(&self) -> impl Future<Output = Result<Bytes>> + Send;

    // 11. touchAsync
    /// Update the last access time of an object in async mode.
    ///
    /// 对应 Java: RFuture<Boolean> touchAsync()
    fn touch_async(&self) -> impl Future<Output = Result<bool>> + Send;

    // 12. migrateAsync(String host, int port, int database, long timeout)
    /// Transfer object from source Redis instance to destination Redis instance in async mode
    ///
    /// 对应 Java: RFuture<Void> migrateAsync(String host, int port, int database, long timeout)
    fn migrate_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send;

    // 13. copyAsync(String host, int port, int database, long timeout)
    /// Copy object from source Redis instance to destination Redis instance in async mode
    ///
    /// 对应 Java: RFuture<Void> copyAsync(String host, int port, int database, long timeout)
    fn copy_to_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send;

    // 14. copyAsync(String destination)
    /// Copy this object instance to the new instance with a defined name.
    ///
    /// 对应 Java: RFuture<Boolean> copyAsync(String destination)
    fn copy_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send;

    // 15. copyAsync(String destination, int database)
    /// Copy this object instance to the new instance with a defined name and database.
    ///
    /// 对应 Java: RFuture<Boolean> copyAsync(String destination, int database)
    fn copy_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send;
    // 16. copyAndReplaceAsync(String destination)
    /// Copy this object instance to the new instance with a defined name, and replace it if it already exists.
    ///
    /// 对应 Java: RFuture<Boolean> copyAndReplaceAsync(String destination)
    fn copy_and_replace_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send;

    // 17. copyAndReplaceAsync(String destination, int database)
    /// Copy this object instance to the new instance with a defined name and database, and replace it if it already exists.
    ///
    /// 对应 Java: RFuture<Boolean> copyAndReplaceAsync(String destination, int database)
    fn copy_and_replace_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send;

    // 18. moveAsync(int database)
    /// Move object to another database in async mode
    ///
    /// 对应 Java: RFuture<Boolean> moveAsync(int database)
    fn move_async(&self, database: i32) -> impl Future<Output = Result<bool>> + Send;

    // 19. deleteAsync
    /// Delete object in async mode
    ///
    /// 对应 Java: RFuture<Boolean> deleteAsync()
    fn delete_async(&self) -> impl Future<Output = Result<bool>> + Send;

    // 20. unlinkAsync
    /// Delete the objects. Actual removal will happen later asynchronously.
    /// Requires Redis 4.0+
    ///
    /// 对应 Java: RFuture<Boolean> unlinkAsync()
    fn unlink_async(&self) -> impl Future<Output = Result<bool>> + Send;

    // 21. renameAsync(String newName)
    /// Rename current object key to newName in async mode
    ///
    /// 对应 Java: RFuture<Void> renameAsync(String newName)
    fn rename_async(&self, new_name: &str) -> impl Future<Output = Result<()>> + Send;

    // 22. renamenxAsync(String newName)
    /// Rename current object key to newName in async mode only if new key is not exists
    ///
    /// 对应 Java: RFuture<Boolean> renamenxAsync(String newName)
    fn renamenx_async(&self, new_name: &str) -> impl Future<Output = Result<bool>> + Send;

    // 23. isExistsAsync
    /// Check object existence in async mode.
    ///
    /// 对应 Java: RFuture<Boolean> isExistsAsync()
    fn is_exists_async(&self) -> impl Future<Output = Result<bool>> + Send;

    // 24. addListenerAsync(ObjectListener listener)
    /// Adds object event listener
    ///
    /// 对应 Java: RFuture<Integer> addListenerAsync(ObjectListener listener)
    fn add_listener_async(&self, listener: Box<dyn ObjectListener + Send + Sync>) -> impl Future<Output = Result<i32>> + Send;

    // 25. removeListenerAsync(int listenerId)
    /// Removes object event listener
    ///
    /// 对应 Java: RFuture<Void> removeListenerAsync(int listenerId)
    fn remove_listener_async(&self, listener_id: i32) -> impl Future<Output = Result<()>> + Send;
}
