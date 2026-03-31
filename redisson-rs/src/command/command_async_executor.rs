use super::command_batch_service::CommandBatchService;
use crate::api::batch_options::BatchOptions;
use crate::api::options::object_params::ObjectParams;
use crate::api::rfuture::RFuture;
use crate::client::codec::codec::{Codec, StringCodec};
use crate::client::protocol::redis_command::RedisCommand;
use crate::client::redis_client::RedisClient;
use crate::client::redis_exception::RedisException;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::master_slave_entry::MasterSlaveEntry;
use crate::connection::service_manager::ServiceManager;
use crate::liveobject::core::redisson_object_builder::RedissonObjectBuilder;
use crate::slot_callback::SlotCallback;
use anyhow::Result;
use bytes::Bytes;
use fred::error::Error;
use fred::types::{FromValue, Key, MultipleKeys, MultipleValues, Value};
use std::any::Any;
use std::future::Future;
use std::sync::Arc;

// ============================================================
// CommandAsyncExecutor — 对应 Java org.redisson.command.CommandAsyncExecutor
//
// 【为什么持有方用泛型 Arc<CE: CommandAsyncExecutor> 而不是 Arc<dyn CommandAsyncExecutor>】
//
//   原因一（不得不用泛型）：
//     本 trait 的方法返回 impl Future（原生 async fn），属于非对象安全 trait，
//     Arc<dyn CommandAsyncExecutor> 根本编译不过。
//     若改用 Arc<dyn>，必须把所有方法签名改成 Pin<Box<dyn Future>>，
//     代价过高且偏离 Rust 惯用写法。
//
//   原因二（同一 runtime 并存多种实现）：
//     CommandAsyncService  — 普通命令，redisson.getBucket() 等持有
//     CommandBatchService  — 批量命令，redisson.createBatch() 持有
//     CommandTransactionService — 事务命令
//     不同对象在同一 runtime 里同时持有不同实现，这和 ConnectionManager
//     "启动时选一种、全局唯一"的模式不同，泛型在调用点编译期确定类型更自然。
// ============================================================

pub enum SyncMode {
    Auto,
    Wait,
    WaitAof,
}

fn unsupported_sync<T>() -> T {
    unimplemented!()
}

fn unsupported_future<T>() -> RFuture<T> {
    Box::pin(async move {
        unimplemented!()
    })
}

pub trait CommandAsyncExecutor: Send + Sync + 'static {
    /// 对应 Java CommandAsyncExecutor.copy(ObjectParams objectParams)
    fn copy_with_object_params(&self, _object_params: ObjectParams) -> Box<dyn Any + Send + Sync> {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.copy(boolean trackChanges)
    fn copy_with_track_changes(&self, _track_changes: bool) -> Box<dyn Any + Send + Sync> {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.getObjectBuilder()
    fn object_builder(&self) -> &RedissonObjectBuilder {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.getConnectionManager()
    fn connection_manager(&self) -> Arc<dyn ConnectionManager>;

    /// 对应 Java CommandAsyncExecutor.getServiceManager()
    fn service_manager(&self) -> &Arc<ServiceManager>;

    /// 对应 Java CommandAsyncExecutor.convertException(ExecutionException e)
    fn convert_exception(&self, _error: anyhow::Error) -> RedisException {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.transfer(CompletionStage<V> future1, CompletableFuture<V> future2)
    fn transfer<V, F>(&self, _future1: F, _future2: tokio::sync::oneshot::Sender<V>)
    where
        V: Send + 'static,
        F: Future<Output = Result<V>> + Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.getNow(CompletableFuture<V> future)
    fn get_now<V>(&self, _future: RFuture<V>) -> Result<V>
    where
        V: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.get(RFuture<V> future)
    fn get_rfuture<V>(&self, _future: RFuture<V>) -> Result<V>
    where
        V: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.get(CompletableFuture<V> future)
    fn get_future<V, F>(&self, _future: F) -> Result<V>
    where
        V: Send + 'static,
        F: Future<Output = Result<V>> + Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.getInterrupted(RFuture<V> future)
    fn get_interrupted_rfuture<V>(&self, _future: RFuture<V>) -> Result<V>
    where
        V: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.getInterrupted(CompletableFuture<V> future)
    fn get_interrupted_future<V, F>(&self, _future: F) -> Result<V>
    where
        V: Send + 'static,
        F: Future<Output = Result<V>> + Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(RedisClient client, Codec codec, RedisCommand<T> command, Object... params)
    fn write_async_with_client<T>(
        &self,
        _client: RedisClient,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params)
    fn write_async_with_entry<T>(
        &self,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(byte[] key, Codec codec, RedisCommand<T> command, Object... params)
    fn write_async_with_bytes_key<T>(
        &self,
        _key: Vec<u8>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(ByteBuf key, Codec codec, RedisCommand<T> command, Object... params)
    fn write_async_with_buf_key<T>(
        &self,
        _key: Bytes,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(RedisClient client, MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_client_entry<T>(
        &self,
        _client: RedisClient,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(RedisClient client, String name, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_client_name<T>(
        &self,
        _client: RedisClient,
        _name: String,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(RedisClient client, byte[] key, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_client_bytes_key<T>(
        &self,
        _client: RedisClient,
        _key: Vec<u8>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(RedisClient client, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_client<T>(
        &self,
        _client: RedisClient,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.executeAllAsync(MasterSlaveEntry entry, RedisCommand<?> command, Object... params)
    fn execute_all_async_with_entry<R>(
        &self,
        _entry: Arc<MasterSlaveEntry>,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.executeAllAsync(RedisCommand<?> command, Object... params)
    fn execute_all_async<R>(
        &self,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.writeAllAsync(RedisCommand<?> command, Object... params)
    fn write_all_async<R>(
        &self,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.writeAllAsync(Codec codec, RedisCommand<?> command, Object... params)
    fn write_all_async_with_codec<R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.readAllAsync(Codec codec, RedisCommand<?> command, Object... params)
    fn read_all_async_with_codec<R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.readAllAsync(RedisCommand<?> command, Object... params)
    fn read_all_async<R>(
        &self,
        _command: RedisCommand<Value>,
        _params: Vec<Value>,
    ) -> Vec<RFuture<R>>
    where
        R: Send + 'static,
    {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadAsync(RedisClient client, String name, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_read_async_with_client_name<T>(
        &self,
        _client: RedisClient,
        _name: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_read_async_with_name<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_read_async_with_entry<T>(
        &self,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadAsync(RedisClient client, MasterSlaveEntry entry, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_read_async_with_client_entry<T>(
        &self,
        _client: RedisClient,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadAsync(ByteBuf key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_read_async_with_buf_key<T>(
        &self,
        _key: Bytes,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java BatchService 类型检查，用于 checkNotBatch()
    fn is_batch(&self) -> bool {
        false
    }

    /// 对应 Java CommandAsyncExecutor.isEvalCacheActive()
    fn is_eval_cache_active(&self) -> bool {
        self.connection_manager().config().use_script_cache
    }

    /// Rust 扩展：是否从 replica 读取（仅 cluster + read_from_slave=true 时为 true）
    fn use_replica_for_reads(&self) -> bool {
        false
    }

    /// 对应 Java CommandAsyncExecutor.evalWriteAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_write_async_with_name<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalWriteAsync(ByteBuf key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_write_async_with_buf_key<T>(
        &self,
        _key: Bytes,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalWriteNoRetryAsync(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_write_no_retry_async<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalWriteAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn eval_write_async_with_entry<T>(
        &self,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(byte[] key, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_bytes_key<T>(
        &self,
        _key: Vec<u8>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(ByteBuf key, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_buf_key<T>(
        &self,
        _key: Bytes,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(String key, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_name<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(String key, Codec codec, RedisCommand<T> command, Object... params)
    fn write_async_with_name<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java 当前 Rust 主用入口：按 key 路由的 readAsync 封装
    fn read_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    /// 对应 Java CommandAsyncExecutor.writeAllVoidAsync(RedisCommand<T> command, Object... params)
    fn write_all_void_async<T>(
        &self,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<()>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeAsync(String key, RedisCommand<T> command, Object... params)
    fn write_async_no_codec<T>(
        &self,
        key: String,
        command: RedisCommand<T>,
        params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        self.write_async_with_name(key, Arc::new(StringCodec), command, params)
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(String key, RedisCommand<T> command, Object... params)
    fn read_async_no_codec<T>(
        &self,
        key: String,
        command: RedisCommand<T>,
        params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        self.read_async_with_name(key, Arc::new(StringCodec), command, params)
    }

    /// 对应 Java CommandAsyncExecutor.readAsync(MasterSlaveEntry entry, Codec codec, RedisCommand<T> command, Object... params)
    fn read_async_with_entry_only<T>(
        &self,
        _entry: Arc<MasterSlaveEntry>,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readRandomAsync(Codec codec, RedisCommand<T> command, Object... params)
    fn read_random_async<T>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.readRandomAsync(RedisClient client, Codec codec, RedisCommand<T> command, Object... params)
    fn read_random_async_with_client<T>(
        &self,
        _client: RedisClient,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.pollFromAnyAsync(String name, Codec codec, RedisCommand<?> command, long secondsTimeout, String... queueNames)
    fn poll_from_any_async(
        &self,
        _name: String,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<Value>,
        _seconds_timeout: i64,
        _queue_names: Vec<String>,
    ) -> RFuture<Value> {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.encode(Codec codec, Object value)
    fn encode(&self, _codec: Arc<dyn Codec>, _value: Value) -> Bytes {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.encodeMapKey(Codec codec, Object value)
    fn encode_map_key(&self, _codec: Arc<dyn Codec>, _value: Value) -> Bytes {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.encodeMapValue(Codec codec, Object value)
    fn encode_map_value(&self, _codec: Arc<dyn Codec>, _value: Value) -> Bytes {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.readBatchedAsync(Codec codec, RedisCommand<T> command, SlotCallback<T, R> callback, Object... keys)
    fn read_batched_async<T, R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _callback: Arc<dyn SlotCallback<T, R>>,
        _keys: Vec<Value>,
    ) -> RFuture<R>
    where
        T: FromValue + Send + 'static,
        R: Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.writeBatchedAsync(Codec codec, RedisCommand<T> command, SlotCallback<T, R> callback, Object... keys)
    fn write_batched_async<T, R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _callback: Arc<dyn SlotCallback<T, R>>,
        _keys: Vec<Value>,
    ) -> RFuture<R>
    where
        T: FromValue + Send + 'static,
        R: Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalWriteBatchedAsync(Codec codec, RedisCommand<T> command, String script, List<Object> keys, SlotCallback<T, R> callback)
    fn eval_write_batched_async<T, R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _callback: Arc<dyn SlotCallback<T, R>>,
    ) -> RFuture<R>
    where
        T: FromValue + Send + 'static,
        R: Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.evalReadBatchedAsync(Codec codec, RedisCommand<T> command, String script, List<Object> keys, SlotCallback<T, R> callback)
    fn eval_read_batched_async<T, R>(
        &self,
        _codec: Arc<dyn Codec>,
        _command: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _callback: Arc<dyn SlotCallback<T, R>>,
    ) -> RFuture<R>
    where
        T: FromValue + Send + 'static,
        R: Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.isEvalShaROSupported()
    fn is_eval_sha_ro_supported(&self) -> bool {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.setEvalShaROSupported(boolean value)
    fn set_eval_sha_ro_supported(&self, _value: bool) {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.syncedEvalWithRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn synced_eval_with_retry<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.syncedEvalNoRetry(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn synced_eval_no_retry<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.syncedEvalNoRetry(long timeout, SyncMode syncMode, String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn synced_eval_no_retry_with_mode<T>(
        &self,
        _timeout: u64,
        _sync_mode: SyncMode,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.syncedEvalWithRetry(long timeout, SyncMode syncMode, String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn synced_eval_with_retry_with_mode<T>(
        &self,
        _timeout: u64,
        _sync_mode: SyncMode,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.syncedEval(String key, Codec codec, RedisCommand<T> evalCommandType, String script, List<Object> keys, Object... params)
    fn synced_eval<T>(
        &self,
        _key: String,
        _codec: Arc<dyn Codec>,
        _eval_command_type: RedisCommand<T>,
        _script: String,
        _keys: Vec<Value>,
        _params: Vec<Value>,
    ) -> RFuture<T>
    where
        T: FromValue + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.handleNoSync(CompletionStage<T> stage, Function<Throwable, CompletionStage<?>> supplier)
    fn handle_no_sync<T, F>(&self, _stage: F) -> RFuture<T>
    where
        T: Send + 'static,
        F: Future<Output = Result<T>> + Send + 'static,
    {
        unsupported_future()
    }

    /// 对应 Java CommandAsyncExecutor.isTrackChanges()
    fn is_track_changes(&self) -> bool {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.createCommandBatchService(BatchOptions options)
    fn create_command_batch_service(&self, _options: BatchOptions) -> CommandBatchService {
        unsupported_sync()
    }

    /// 对应 Java CommandAsyncExecutor.create(ConnectionManager, RedissonObjectBuilder, ReferenceType)
    fn create(
        _connection_manager: Arc<dyn ConnectionManager>,
        _object_builder: RedissonObjectBuilder,
        _reference_type: crate::liveobject::core::redisson_object_builder::ReferenceType,
    ) -> Box<dyn Any + Send + Sync>
    where
        Self: Sized,
    {
        unsupported_sync()
    }

    /// 对应 Java 当前 Rust 主用入口：按 key 路由的 writeAsync 封装
    fn write_async<T, K, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        args: Vec<R>,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        R: TryInto<Value> + Send + 'static,
        R::Error: Into<Error> + Send;

    /// 对应 Java 当前 Rust 主用入口：按 key 路由的 evalWriteAsync 封装
    fn eval_write_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send + 'static,
        R::Error: Into<Error> + Send;

    /// 对应 Java 当前 Rust 主用入口：按 key 路由的 evalReadAsync 封装
    fn eval_read_async<T, K, MK, R>(
        &self,
        key: K,
        command: RedisCommand<T>,
        script: &str,
        keys: MK,
        args: R,
    ) -> impl Future<Output = Result<T>> + Send + 'static
    where
        T: FromValue + Send + 'static,
        K: Into<Key> + Send,
        MK: Into<MultipleKeys> + Send,
        R: TryInto<MultipleValues> + Send + 'static,
        R::Error: Into<Error> + Send;
}
