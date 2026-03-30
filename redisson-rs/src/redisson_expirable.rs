use fred::types::Value;
use crate::api::object_encoding::ObjectEncoding;
use crate::api::object_listener::ObjectListener;
use crate::api::rexpirable::RExpirable;
use crate::api::rexpirable_async::RExpirableAsync;
use crate::api::robject_async::RObjectAsync;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::client::protocol::redis_commands as commands;
use crate::ext::RedisKey;
use crate::redisson_object::RedissonObject;
use anyhow::Result;
use bytes::Bytes;
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

// ============================================================
// RedissonExpirable — 对应 Java org.redisson.RedissonExpirable
// ============================================================

/// 对应 Java abstract class RedissonExpirable extends RedissonObject implements RExpirable, RExpirableAsync
pub struct RedissonExpirable<CE: CommandAsyncExecutor> {
    pub(crate) base: RedissonObject<CE>,
}

impl<CE: CommandAsyncExecutor> Deref for RedissonExpirable<CE> {
    type Target = RedissonObject<CE>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<CE: CommandAsyncExecutor> RedissonExpirable<CE> {
    /// 对应 Java RedissonExpirable(CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<CE>, name: impl RedisKey) -> Self {
        Self {
            base: RedissonObject::new(command_executor, name),
        }
    }
}

// ============================================================
// impl RObjectAsync for RedissonExpirable（委托给 RedissonObject）
// 方法顺序与 Java RObjectAsync 保持一致
// ============================================================

impl<CE: CommandAsyncExecutor> RObjectAsync for RedissonExpirable<CE> {
    fn get_name(&self) -> String {
        self.base.get_name()
    }

    // 1. getIdleTimeAsync
    fn get_idle_time_async(&self) -> impl Future<Output = Result<i64>> + Send {
        self.base.get_idle_time_async()
    }

    // 2. getReferenceCountAsync
    fn get_reference_count_async(&self) -> impl Future<Output = Result<i32>> + Send {
        self.base.get_reference_count_async()
    }

    // 3. getAccessFrequencyAsync
    fn get_access_frequency_async(&self) -> impl Future<Output = Result<i32>> + Send {
        self.base.get_access_frequency_async()
    }

    // 4. getInternalEncodingAsync
    fn get_internal_encoding_async(&self) -> impl Future<Output = Result<ObjectEncoding>> + Send {
        self.base.get_internal_encoding_async()
    }

    // 5. sizeInMemoryAsync
    fn size_in_memory_async(&self) -> impl Future<Output = Result<i64>> + Send {
        self.base.size_in_memory_async()
    }

    // 6. restoreAsync(byte[] state)
    fn restore_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_async(state)
    }

    // 7. restoreAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_with_ttl_async(state, time_to_live)
    }

    // 8. restoreAndReplaceAsync(byte[] state)
    fn restore_and_replace_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_and_replace_async(state)
    }

    // 9. restoreAndReplaceAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    fn restore_and_replace_with_ttl_async(&self, state: Bytes, time_to_live: Duration) -> impl Future<Output = Result<()>> + Send {
        self.base.restore_and_replace_with_ttl_async(state, time_to_live)
    }

    // 10. dumpAsync
    fn dump_async(&self) -> impl Future<Output = Result<Bytes>> + Send {
        self.base.dump_async()
    }

    // 11. touchAsync
    fn touch_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.touch_async()
    }

    // 12. migrateAsync(String host, int port, int database, long timeout)
    fn migrate_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send {
        self.base.migrate_async(host, port, database, timeout)
    }

    // 13. copyAsync(String host, int port, int database, long timeout)
    fn copy_to_async(&self, host: &str, port: i32, database: i32, timeout: u64) -> impl Future<Output = Result<()>> + Send {
        self.base.copy_to_async(host, port, database, timeout)
    }

    // 14. copyAsync(String destination)
    fn copy_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_async(destination)
    }

    // 15. copyAsync(String destination, int database)
    fn copy_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_to_database_async(destination, database)
    }

    // 16. copyAndReplaceAsync(String destination)
    fn copy_and_replace_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_and_replace_async(destination)
    }

    // 17. copyAndReplaceAsync(String destination, int database)
    fn copy_and_replace_to_database_async(&self, destination: &str, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.copy_and_replace_to_database_async(destination, database)
    }

    // 18. moveAsync(int database)
    fn move_async(&self, database: i32) -> impl Future<Output = Result<bool>> + Send {
        self.base.move_async(database)
    }

    // 19. deleteAsync
    fn delete_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.delete_async()
    }

    // 20. unlinkAsync
    fn unlink_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.unlink_async()
    }

    // 21. renameAsync(String newName)
    fn rename_async(&self, new_name: &str) -> impl Future<Output = Result<()>> + Send {
        self.base.rename_async(new_name)
    }

    // 22. renamenxAsync(String newName)
    fn renamenx_async(&self, new_name: &str) -> impl Future<Output = Result<bool>> + Send {
        self.base.renamenx_async(new_name)
    }

    // 23. isExistsAsync
    fn is_exists_async(&self) -> impl Future<Output = Result<bool>> + Send {
        self.base.is_exists_async()
    }

    // 24. addListenerAsync(ObjectListener listener)
    fn add_listener_async(&self, listener: Box<dyn ObjectListener + Send + Sync>) -> impl Future<Output = Result<i32>> + Send {
        self.base.add_listener_async(listener)
    }

    // 25. removeListenerAsync(int listenerId)
    fn remove_listener_async(&self, listener_id: i32) -> impl Future<Output = Result<()>> + Send {
        self.base.remove_listener_async(listener_id)
    }
}

// ============================================================
// impl RExpirableAsync for RedissonExpirable
// ============================================================

impl<CE: CommandAsyncExecutor> RExpirableAsync for RedissonExpirable<CE> {
    fn expire(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let millis = duration.as_millis() as u64;
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIRE, vec![Value::from(name.clone()), Value::from(millis.to_string())])
                .await
        }
    }

    fn expire_at(&self, timestamp_millis: u64) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIREAT, vec![Value::from(name.clone()), Value::from(timestamp_millis.to_string())])
                .await
        }
    }

    fn expire_if_set(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let millis = duration.as_millis() as u64;
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIRE, vec![Value::from(name.clone()), Value::from(millis.to_string()), Value::from("XX")])
                .await
        }
    }

    fn expire_if_not_set(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let millis = duration.as_millis() as u64;
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIRE, vec![Value::from(name.clone()), Value::from(millis.to_string()), Value::from("NX")])
                .await
        }
    }

    fn expire_if_greater(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let millis = duration.as_millis() as u64;
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIRE, vec![Value::from(name.clone()), Value::from(millis.to_string()), Value::from("GT")])
                .await
        }
    }

    fn expire_if_less(&self, duration: Duration) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let millis = duration.as_millis() as u64;
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PEXPIRE, vec![Value::from(name.clone()), Value::from(millis.to_string()), Value::from("LT")])
                .await
        }
    }

    fn clear_expire(&self) -> impl Future<Output = Result<bool>> + Send {
        async {
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .write_async(&name, commands::PERSIST, vec![Value::from(name.clone())])
                .await
        }
    }

    fn remain_time_to_live(&self) -> impl Future<Output = Result<i64>> + Send {
        async {
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .read_async(&name, commands::PTTL, vec![Value::from(name.clone())])
                .await
        }
    }

    fn get_expire_time(&self) -> impl Future<Output = Result<i64>> + Send {
        async {
            let name = self.base.name.read().clone();
            self.base
                .command_executor
                .read_async(&name, commands::PEXPIRETIME, vec![Value::from(name.clone())])
                .await
        }
    }
}

// ============================================================
// impl RExpirable for RedissonExpirable (空实现，继承自 RExpirableAsync)
// ============================================================

impl<CE: CommandAsyncExecutor> RExpirable for RedissonExpirable<CE> {}
