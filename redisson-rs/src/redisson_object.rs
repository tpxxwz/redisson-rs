use crate::api::object_encoding::ObjectEncoding;
use crate::api::object_listener::ObjectListener;
use crate::api::robject_async::RObjectAsync;
use crate::client::protocol::redis_commands as commands;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::ext::RedisKey;
use anyhow::Result;
use bytes::Bytes;
use dashmap::DashMap;
use fred::types::Value;
use parking_lot::RwLock;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

// ============================================================
// RedissonObject — 对应 Java org.redisson.RedissonObject
// ============================================================

/// 对应 Java RedissonObject.prefixName(prefix, name)
pub(crate) fn prefix_name(prefix: &str, name: &str) -> String {
    if name.contains('{') {
        format!("{}:{}", prefix, name)
    } else {
        format!("{}:{{{}}}", prefix, name)
    }
}

/// 对应 Java RedissonObject.suffixName(name, suffix)
pub(crate) fn suffix_name(name: &str, suffix: &str) -> String {
    if name.contains('{') {
        format!("{}:{}", name, suffix)
    } else {
        format!("{{{}}}:{}", name, suffix) // Fix: was format!("{{}}:{}", suffix) — name was missing
    }
}

/// Redisson 基础对象
/// 对应 Java abstract class RedissonObject implements RObject, RObjectAsync
pub struct RedissonObject<CE: CommandAsyncExecutor> {
    /// 对应 Java RedissonObject.commandExecutor
    pub(crate) command_executor: Arc<CE>,
    /// 对应 Java RedissonObject.name — RwLock 支持 rename 后更新名称（interior mutability）
    pub(crate) name: RwLock<String>,
    /// 对应 Java RedissonObject.listeners
    pub(crate) listeners: DashMap<String, Vec<i32>>,
}

impl<CE: CommandAsyncExecutor> RedissonObject<CE> {
    /// 对应 Java RedissonObject(Codec codec, CommandAsyncExecutor commandExecutor, String name)
    pub fn new(command_executor: &Arc<CE>, name: impl RedisKey) -> Self {
        let mapped = command_executor
            .service_manager()
            .name_mapper
            .map(&name.key());
        Self {
            command_executor: command_executor.clone(),
            name: RwLock::new(mapped),
            listeners: DashMap::new(),
        }
    }

    /// 对应 Java RedissonObject.getServiceManager()
    pub fn get_service_manager(&self) -> &Arc<crate::connection::service_manager::ServiceManager> {
        self.command_executor.service_manager()
    }

    /// 对应 Java RedissonObject.getRawName()
    pub fn get_raw_name(&self) -> String {
        self.name.read().clone()
    }

    /// 对应 Java RedissonObject.getRawName(Object o)
    /// 默认忽略参数，子类可覆盖（如 RedissonMap 按 key 返回不同名称）
    pub(crate) fn get_raw_name_for<T>(&self, _o: &T) -> String {
        self.get_raw_name()
    }

    /// 对应 Java RedissonObject.setName(String name)
    /// 对应 Java 行为：先 mapName 再存储
    pub fn set_name(&self, name: String) {
        let mapped = self
            .command_executor
            .service_manager()
            .name_mapper
            .map(&name);
        *self.name.write() = mapped;
    }

    /// 对应 Java RedissonObject.mapName(String name)
    pub(crate) fn map_name(&self, name: &str) -> String {
        self.command_executor
            .service_manager()
            .name_mapper
            .map(name)
    }

    /// 对应 Java RedissonObject.checkNotBatch()
    /// 在 batch 模式下抛出错误
    pub(crate) fn check_not_batch(&self) -> Result<()> {
        if self.command_executor.is_batch() {
            anyhow::bail!("This method doesn't work in batch mode.");
        }
        Ok(())
    }

    /// 对应 Java RedissonObject.sizeInMemoryAsync(List<Object> keys)
    pub async fn size_in_memory_async_for_keys(
        &self,
        keys: Vec<fred::types::Key>,
    ) -> Result<i64> {
        Self::size_in_memory_async_with_executor(&self.command_executor, keys).await
    }

    /// 对应 Java RedissonObject.sizeInMemoryAsync(CommandAsyncExecutor, List<Object> keys)
    pub async fn size_in_memory_async_with_executor<CE2: CommandAsyncExecutor>(
        executor: &Arc<CE2>,
        keys: Vec<fred::types::Key>,
    ) -> Result<i64> {
        let script = "
            local total = 0;
            for j = 1, #KEYS, 1 do
                local size = redis.call('memory', 'usage', KEYS[j]);
                if size ~= false then
                    total = total + size;
                end;
            end;
            return total;
        ";
        let routing_key = keys.first().cloned().unwrap_or_else(|| fred::types::Key::from(""));
        executor
            .eval_write_async(routing_key, commands::EVAL_LONG, script, keys, Vec::<Value>::new())
            .await
    }
}

// ============================================================
// impl RObjectAsync for RedissonObject
// 方法顺序与 Java RObjectAsync 保持一致
// ============================================================

impl<CE: CommandAsyncExecutor> RObjectAsync for RedissonObject<CE> {
    /// 对应 Java RedissonObject.getName()
    fn get_name(&self) -> String {
        self.command_executor
            .service_manager()
            .name_mapper
            .unmap(&self.name.read())
    }

    // 1. getIdleTimeAsync
    fn get_idle_time_async(&self) -> impl Future<Output = Result<i64>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .read_async(&name, commands::OBJECT_IDLETIME, vec![Value::from(name.clone())])
                .await
        }
    }

    // 2. getReferenceCountAsync
    fn get_reference_count_async(&self) -> impl Future<Output = Result<i32>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .read_async(&name, commands::OBJECT_REFCOUNT, vec![Value::from(name.clone())])
                .await
        }
    }

    // 3. getAccessFrequencyAsync
    fn get_access_frequency_async(&self) -> impl Future<Output = Result<i32>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .read_async(&name, commands::OBJECT_FREQ, vec![Value::from(name.clone())])
                .await
        }
    }

    // 4. getInternalEncodingAsync
    fn get_internal_encoding_async(&self) -> impl Future<Output = Result<ObjectEncoding>> + Send {
        async {
            let name = self.get_raw_name();
            let encoding: String = self
                .command_executor
                .read_async(&name, commands::OBJECT_ENCODING, vec![Value::from(name.clone())])
                .await?;
            Ok(ObjectEncoding::value_of_encoding(Some(encoding.as_str())))
        }
    }

    // 5. sizeInMemoryAsync
    fn size_in_memory_async(&self) -> impl Future<Output = Result<i64>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(&name, commands::MEMORY_USAGE, vec![Value::from(name.clone())])
                .await
        }
    }

    // 6. restoreAsync(byte[] state)
    /// 对应 Java: restoreAsync(state) → RESTORE key 0 state
    fn restore_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::RESTORE,
                    vec![Value::from(name.clone()), Value::from(0u64.to_string()), Value::from(state)],
                )
                .await
        }
    }

    // 7. restoreAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    /// 对应 Java: RESTORE key ttl state
    fn restore_with_ttl_async(
        &self,
        state: Bytes,
        time_to_live: Duration,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let name = self.get_raw_name();
            let ttl_ms = time_to_live.as_millis() as u64;
            self.command_executor
                .write_async(
                    &name,
                    commands::RESTORE,
                    vec![Value::from(name.clone()), Value::from(ttl_ms.to_string()), Value::from(state)],
                )
                .await
        }
    }

    // 8. restoreAndReplaceAsync(byte[] state)
    /// 对应 Java: RESTORE key 0 state REPLACE
    fn restore_and_replace_async(&self, state: Bytes) -> impl Future<Output = Result<()>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::RESTORE,
                    vec![
                        Value::from(name.clone()),
                        Value::from(0u64.to_string()),
                        Value::from(state),
                        Value::from("REPLACE"),
                    ],
                )
                .await
        }
    }

    // 9. restoreAndReplaceAsync(byte[] state, long timeToLive, TimeUnit timeUnit)
    /// 对应 Java: RESTORE key ttl state REPLACE
    fn restore_and_replace_with_ttl_async(
        &self,
        state: Bytes,
        time_to_live: Duration,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let name = self.get_raw_name();
            let ttl_ms = time_to_live.as_millis() as u64;
            self.command_executor
                .write_async(
                    &name,
                    commands::RESTORE,
                    vec![
                        Value::from(name.clone()),
                        Value::from(ttl_ms.to_string()),
                        Value::from(state),
                        Value::from("REPLACE"),
                    ],
                )
                .await
        }
    }

    // 10. dumpAsync
    fn dump_async(&self) -> impl Future<Output = Result<Bytes>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .read_async(&name, commands::DUMP, vec![Value::from(name.clone())])
                .await
        }
    }

    // 11. touchAsync
    fn touch_async(&self) -> impl Future<Output = Result<bool>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(&name, commands::TOUCH, vec![Value::from(name.clone())])
                .await
        }
    }

    // 12. migrateAsync(String host, int port, int database, long timeout)
    /// 对应 Java: writeAsync(getRawName(), ..., MIGRATE, host, port, getRawName(), database, timeout)
    fn migrate_async(
        &self,
        host: &str,
        port: i32,
        database: i32,
        timeout: u64,
    ) -> impl Future<Output = Result<()>> + Send {
        let host = host.to_string();
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::MIGRATE,
                    vec![
                        Value::from(host),
                        Value::from(port.to_string()),
                        Value::from(name.clone()),
                        Value::from(database.to_string()),
                        Value::from(timeout.to_string()),
                    ],
                )
                .await
        }
    }

    // 13. copyAsync(String host, int port, int database, long timeout)
    /// 对应 Java: writeAsync(getRawName(), ..., MIGRATE, host, port, getRawName(), database, timeout, "COPY")
    fn copy_to_async(
        &self,
        host: &str,
        port: i32,
        database: i32,
        timeout: u64,
    ) -> impl Future<Output = Result<()>> + Send {
        let host = host.to_string();
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::MIGRATE,
                    vec![
                        Value::from(host),
                        Value::from(port.to_string()),
                        Value::from(name.clone()),
                        Value::from(database.to_string()),
                        Value::from(timeout.to_string()),
                        Value::from("COPY"),
                    ],
                )
                .await
        }
    }

    // 14. copyAsync(String destination)
    fn copy_async(&self, destination: &str) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(&name, commands::COPY, vec![Value::from(name.clone()), Value::from(destination)])
                .await
        }
    }

    // 15. copyAsync(String destination, int database)
    fn copy_to_database_async(
        &self,
        destination: &str,
        database: i32,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::COPY,
                    vec![
                        Value::from(name.clone()),
                        Value::from(destination),
                        Value::from("DB"),
                        Value::from(database.to_string()),
                    ],
                )
                .await
        }
    }

    // 16. copyAndReplaceAsync(String destination)
    fn copy_and_replace_async(
        &self,
        destination: &str,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::COPY,
                    vec![Value::from(name.clone()), Value::from(destination), Value::from("REPLACE")],
                )
                .await
        }
    }

    // 17. copyAndReplaceAsync(String destination, int database)
    fn copy_and_replace_to_database_async(
        &self,
        destination: &str,
        database: i32,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::COPY,
                    vec![
                        Value::from(name.clone()),
                        Value::from(destination),
                        Value::from("DB"),
                        Value::from(database.to_string()),
                        Value::from("REPLACE"),
                    ],
                )
                .await
        }
    }

    // 18. moveAsync(int database)
    fn move_async(&self, database: i32) -> impl Future<Output = Result<bool>> + Send {
        async move {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(
                    &name,
                    commands::MOVE,
                    vec![Value::from(name.clone()), Value::from(database.to_string())],
                )
                .await
        }
    }

    // 19. deleteAsync
    /// 对应 Java: DEL_BOOL (BooleanNullSafeReplayConvertor)
    fn delete_async(&self) -> impl Future<Output = Result<bool>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(&name, commands::DEL_BOOL, vec![Value::from(name.clone())])
                .await
        }
    }

    // 20. unlinkAsync
    /// 对应 Java: UNLINK_BOOL (BooleanNullSafeReplayConvertor)
    fn unlink_async(&self) -> impl Future<Output = Result<bool>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .write_async(&name, commands::UNLINK_BOOL, vec![Value::from(name.clone())])
                .await
        }
    }

    // 21. renameAsync(String newName)
    /// 对应 Java: RENAME，成功后更新 self.name
    /// Cluster 模式下跨 slot 时用 dump → restore → delete fallback
    fn rename_async(&self, new_name: &str) -> impl Future<Output = Result<()>> + Send {
        let new_name_owned = new_name.to_string();
        async move {
            let nn = self.map_name(&new_name_owned);
            let old_name = self.get_raw_name();
            let connection_manager = self.command_executor.connection_manager();

            // 非 cluster 或同 slot：直接 RENAME
            if !self.get_service_manager().is_cluster_config()
                || connection_manager.calc_slot(nn.as_bytes())
                    == connection_manager.calc_slot(old_name.as_bytes())
            {
                let result = self
                    .command_executor
                    .write_async(
                        &old_name,
                        commands::RENAME,
                        vec![Value::from(old_name.clone()), Value::from(nn.clone())],
                    )
                    .await;
                if result.is_ok() {
                    self.set_name(new_name_owned);
                }
                return result;
            }

            // Cluster 跨 slot：batch 模式不支持
            self.check_not_batch()?;

            // dump → restore → delete fallback
            let state = self.dump_async().await?;
            self.command_executor
                .write_async(
                    &nn,
                    commands::RESTORE,
                    vec![
                        Value::from(nn.clone()),
                        Value::from(0u64.to_string()),
                        Value::from(state),
                        Value::from("REPLACE"),
                    ],
                )
                .await?;
            self.delete_async().await?;
            self.set_name(new_name_owned);
            Ok(())
        }
    }

    // 22. renamenxAsync(String newName)
    /// 对应 Java: RENAMENX，成功（true）后更新 self.name
    fn renamenx_async(&self, new_name: &str) -> impl Future<Output = Result<bool>> + Send {
        let new_name_owned = new_name.to_string();
        async move {
            let old_name = self.get_raw_name();
            let result: Result<bool> = self
                .command_executor
                .write_async(
                    &old_name,
                    commands::RENAMENX,
                    vec![Value::from(old_name.clone()), Value::from(new_name_owned.clone())],
                )
                .await;
            if let Ok(true) = result {
                self.set_name(new_name_owned);
            }
            result
        }
    }

    // 23. isExistsAsync
    fn is_exists_async(&self) -> impl Future<Output = Result<bool>> + Send {
        async {
            let name = self.get_raw_name();
            self.command_executor
                .read_async(&name, commands::EXISTS, vec![Value::from(name.clone())])
                .await
        }
    }

    // 24. addListenerAsync(ObjectListener listener)
    // TODO: 需要集成 EventListenerService 到 ServiceManager
    fn add_listener_async(
        &self,
        _listener: Box<dyn ObjectListener + Send + Sync>,
    ) -> impl Future<Output = Result<i32>> + Send {
        async {
            anyhow::bail!(
                "addListenerAsync not implemented yet; requires EventListenerService integration"
            )
        }
    }

    // 25. removeListenerAsync(int listenerId)
    // TODO: 需要集成 EventListenerService 到 ServiceManager
    fn remove_listener_async(&self, _listener_id: i32) -> impl Future<Output = Result<()>> + Send {
        async {
            anyhow::bail!(
                "removeListenerAsync not implemented yet; requires EventListenerService integration"
            )
        }
    }
}
