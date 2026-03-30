use super::command_async_executor::CommandAsyncExecutor;
use crate::client::protocol::convertor::Convertor;
use crate::client::protocol::redis_command::RedisCommand;
use crate::connection::connection_manager::ConnectionManager;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use anyhow::Result;
use fred::error::Error;
use fred::interfaces::{ClientLike, KeysInterface};
use fred::prelude::Pool;
use fred::types::{ClusterHash, CustomCommand, Expiration, FromValue, Key, MultipleKeys, MultipleValues, Value};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// ============================================================
// CommandAsyncService — 对应 Java org.redisson.command.CommandAsyncService
// ============================================================

/// 对应 Java SORT_RO_SUPPORTED：Redis >= 7.0 支持只读 SORT
static SORT_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);
/// 对应 Java EVALSHA_RO_SUPPORTED：Redis >= 7.0 支持 EVALSHA_RO
static EVALSHA_RO_SUPPORTED: AtomicBool = AtomicBool::new(true);
/// 对应 Java EVALSHA_SUPPORTED：Redis >= 7.0 支持 EVALSHA
static EVALSHA_SUPPORTED: AtomicBool = AtomicBool::new(true);

pub struct CommandAsyncService {
    pub(crate) connection_manager: Arc<FredConnectionManager>,
}

impl CommandAsyncService {
    pub fn new(connection_manager: Arc<FredConnectionManager>) -> Self {
        Self { connection_manager }
    }

    pub(crate) fn pool(&self) -> &Pool {
        &self.connection_manager.pool
    }

    pub async fn set_value(
        &self,
        key: &str,
        value: String,
        expire: Option<Expiration>,
    ) -> Result<()> {
        self.connection_manager
            .pool
            .set::<(), _, _>(key, value, expire, None, false)
            .await?;
        Ok(())
    }

    pub async fn get_str(&self, key: &str) -> Result<Option<String>> {
        Ok(self.connection_manager.pool.get(key).await?)
    }
}

// ============================================================
// 内部执行逻辑
// ============================================================

impl CommandAsyncService {
    fn is_unknown_command_error(err: &anyhow::Error) -> bool {
        err.to_string().contains("ERR unknown command")
    }

    fn is_noscript_error(err: &anyhow::Error) -> bool {
        err.to_string().contains("NOSCRIPT")
    }

    /// 将 sub_name + args 组装成 all_args，对应 Java 的 params 数组构建逻辑
    fn build_args<R>(sub_name: Option<&'static str>, args: Vec<R>) -> Result<Vec<Value>>
    where
        R: TryInto<Value>,
        R::Error: Into<Error>,
    {
        let mut all_args: Vec<Value> = Vec::new();
        if let Some(sub) = sub_name {
            all_args.push(sub.into());
        }
        for arg in args {
            all_args.push(arg.try_into().map_err(|e| anyhow::anyhow!("{:?}", e.into()))?);
        }
        Ok(all_args)
    }

    /// 将 EVAL 脚本 + keys + args 组装成 all_args
    /// 格式：[script, numkeys, key1, key2, ..., arg1, arg2, ...]
    fn build_eval_args(script: String, lua_keys: Vec<Key>, args_vec: Vec<Value>) -> Vec<Value> {
        let mut all_args: Vec<Value> = Vec::with_capacity(2 + lua_keys.len() + args_vec.len());
        all_args.push(Value::from(script));
        all_args.push(Value::from(lua_keys.len() as i64));
        for k in lua_keys { all_args.push(k.into()); }
        for arg in args_vec { all_args.push(arg); }
        all_args
    }

    /// 对应 Java CommandAsyncService.async()
    /// 处理 SORT_RO fallback（Redis 7.0+ 只读 SORT）。
    ///
    /// TODO(ignoreRedirect): Java 的 ignoreRedirect 参数防止 Batch 模式下命令跟随 MOVED/ASK
    ///   redirect 分裂到不同节点。fred 的 Pool 没有暴露 ignoreRedirect 选项，
    ///   batch 模式下用 ClusterHash::Custom 仍有跟随 redirect 分裂的风险。
    async fn async_inner<T>(
        pool: Pool,
        read_only_mode: bool,
        slot: ClusterHash,
        command: RedisCommand<T>,
        all_args: Vec<Value>,
    ) -> Result<T>
    where
        T: FromValue,
    {
        // Java 逻辑：
        // if (readOnlyMode && SORT && !SORT_RO_SUPPORTED) { readOnlyMode=false; } // 不 return，fall through
        // else if (readOnlyMode && SORT && SORT_RO_SUPPORTED) { try SORT_RO; if fail -> set flag and SORT; return; }
        if read_only_mode && command.name == "SORT" {
            if !SORT_RO_SUPPORTED.load(Ordering::Relaxed) {
                // !flag 时 Java 只改 readOnlyMode=false，然后 fall through 到普通 SORT 路径（不 return）
            } else {
                let sort_ro_cmd = CustomCommand::new_static("SORT_RO", slot.clone(), false);
                match Self::exec(&pool, true, sort_ro_cmd, all_args.clone(), command.convertor).await {
                    Ok(v) => return Ok(v),
                    Err(e) if Self::is_unknown_command_error(&e) => {
                        SORT_RO_SUPPORTED.store(false, Ordering::Relaxed);
                        let sort_cmd = CustomCommand::new_static(command.name, slot, false);
                        return Self::exec(&pool, false, sort_cmd, all_args, command.convertor).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let cmd = CustomCommand::new_static(command.name, slot, false);
        Self::exec(&pool, read_only_mode, cmd, all_args, command.convertor).await
    }

    /// 对应 Java CommandAsyncService.evalAsync()
    /// 1. 优先尝试 EVALSHA_RO（readOnly=true, Redis 7.0+）或 EVALSHA（isEvalCacheActive=true 时）
    /// 2. NOSCRIPT → SCRIPT LOAD 重试
    /// 3. ERR unknown command → 标记降级，走完整 EVAL
    ///
    /// TODO(ignoreRedirect): Java 的 NOSCRIPT fallback 调用 asyncNoScript(..., ignoreRedirect=false, noRetry)，
    ///   其中 ignoreRedirect 防止 batch 模式下跟随 redirect 分裂节点。fred 无此选项。
    async fn eval_async_inner<T>(
        pool: Pool,
        read_only_mode: bool,
        slot: ClusterHash,
        command: RedisCommand<T>,
        script: String,
        lua_keys: Vec<Key>,
        args_vec: Vec<Value>,
        use_script_cache: bool,
    ) -> Result<T>
    where
        T: FromValue,
    {
        // 对应 Java isEvalCacheActive() == false 时：跳过 EVALSHA，直接走完整 EVAL
        if !use_script_cache {
            let all_args = Self::build_eval_args(script, lua_keys, args_vec);
            let cmd = CustomCommand::new_static(command.name, slot, false);
            return Self::exec(&pool, read_only_mode, cmd, all_args, command.convertor).await;
        }

        let build_eval_all_args = |sha_or_script: &str| -> Vec<Value> {
            let mut all_args = Vec::with_capacity(2 + lua_keys.len() + args_vec.len());
            all_args.push(Value::from(sha_or_script));
            all_args.push(Value::from(lua_keys.len() as i64));
            for k in &lua_keys { all_args.push(k.clone().into()); }
            for arg in &args_vec { all_args.push(arg.clone()); }
            all_args
        };

        // 情况1：readOnly + EVALSHA_RO_SUPPORTED → 尝试 EVALSHA_RO
        if read_only_mode && EVALSHA_RO_SUPPORTED.load(Ordering::Relaxed) {
            let sha = Self::sha1_hash(&script);
            let all_args = build_eval_all_args(&sha);
            let cmd = CustomCommand::new_static("EVALSHA_RO", slot.clone(), false);
            match Self::exec(&pool, true, cmd, all_args, command.convertor).await {
                Ok(v) => return Ok(v),
                Err(e) if Self::is_unknown_command_error(&e) => {
                    EVALSHA_RO_SUPPORTED.store(false, Ordering::Relaxed);
                }
                Err(e) if Self::is_noscript_error(&e) => {
                    // script 未缓存，fallthrough 到 EVALSHA
                }
                Err(e) => return Err(e),
            }
        }

        // 情况2：尝试 EVALSHA
        if EVALSHA_SUPPORTED.load(Ordering::Relaxed) {
            let sha = Self::sha1_hash(&script);
            let all_args = build_eval_all_args(&sha);
            let cmd = CustomCommand::new_static("EVALSHA", slot.clone(), false);
            match Self::exec(&pool, read_only_mode, cmd, all_args, command.convertor).await {
                Ok(v) => return Ok(v),
                Err(e) if Self::is_unknown_command_error(&e) => {
                    EVALSHA_SUPPORTED.store(false, Ordering::Relaxed);
                }
                Err(e) if Self::is_noscript_error(&e) => {
                    let sha = Self::script_load(&pool, &slot, &script).await?;
                    let all_args = build_eval_all_args(&sha);
                    let cmd = CustomCommand::new_static("EVALSHA", slot, false);
                    return Self::exec(&pool, read_only_mode, cmd, all_args, command.convertor).await;
                }
                Err(e) => return Err(e),
            }
        }

        // 情况3：降级到完整 EVAL（每次发完整脚本）
        let all_args = Self::build_eval_args(script, lua_keys, args_vec);
        let cmd = CustomCommand::new_static(command.name, slot, false);
        Self::exec(&pool, read_only_mode, cmd, all_args, command.convertor).await
    }

    /// 实际执行 pool.custom() / pool.replicas().custom()，并应用 convertor
    async fn exec<T>(
        pool: &Pool,
        use_replica: bool,
        cmd: CustomCommand,
        all_args: Vec<Value>,
        convertor: Option<&'static dyn Convertor<T>>,
    ) -> Result<T>
    where
        T: FromValue,
    {
        if use_replica {
            if let Some(conv) = convertor {
                let raw: Value = pool.replicas().custom(cmd, all_args).await?;
                conv.convert(raw)
            } else {
                Ok(pool.replicas().custom(cmd, all_args).await?)
            }
        } else if let Some(conv) = convertor {
            let raw: Value = pool.custom(cmd, all_args).await?;
            conv.convert(raw)
        } else {
            Ok(pool.custom(cmd, all_args).await?)
        }
    }

    /// 在指定 slot 上执行 SCRIPT LOAD，返回 SHA
    async fn script_load(pool: &Pool, slot: &ClusterHash, script: &str) -> Result<String> {
        let cmd = CustomCommand::new_static("SCRIPT", slot.clone(), false);
        let all_args: Vec<Value> = vec![Value::from("LOAD"), Value::from(script)];
        let raw: Value = pool.custom(cmd, all_args).await?;
        Ok(<String as FromValue>::from_value(raw)?)
    }

    /// 计算 SHA-1 哈希，用于 EVALSHA/EVALSHA_RO
    fn sha1_hash(script: &str) -> String {
        fred::util::sha1_hash(script)
    }
}

// ============================================================
// CommandAsyncExecutor impl
// ============================================================

impl CommandAsyncExecutor for CommandAsyncService {
    fn connection_manager(&self) -> Arc<dyn ConnectionManager> {
        self.connection_manager.clone()
    }

    fn service_manager(&self) -> &Arc<ServiceManager> {
        self.connection_manager.service_manager()
    }

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
        R::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let use_replica = self.connection_manager.use_replica_for_reads;
        let slot = ClusterHash::Custom(self.connection_manager.calc_slot(key.into().as_bytes()));
        async move {
            let all_args = Self::build_args(command.sub_name, args)?;
            Self::async_inner(pool, use_replica, slot, command, all_args).await
        }
    }

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
        R::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let slot = ClusterHash::Custom(self.connection_manager.calc_slot(key.into().as_bytes()));
        async move {
            let all_args = Self::build_args(command.sub_name, args)?;
            Self::async_inner(pool, false, slot, command, all_args).await
        }
    }

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
        R::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let slot = ClusterHash::Custom(self.connection_manager.calc_slot(key.into().as_bytes()));
        let script = script.to_string();
        let lua_keys: Vec<Key> = keys.into().inner();
        let use_script_cache = self.is_eval_cache_active();
        async move {
            let args_val = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", Into::<Error>::into(e)))?;
            Self::eval_async_inner(pool, false, slot, command, script, lua_keys, args_val.into_array(), use_script_cache).await
        }
    }

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
        R::Error: Into<Error> + Send,
    {
        let pool = self.connection_manager.pool.clone();
        let use_replica = self.connection_manager.use_replica_for_reads;
        let slot = ClusterHash::Custom(self.connection_manager.calc_slot(key.into().as_bytes()));
        let script = script.to_string();
        let lua_keys: Vec<Key> = keys.into().inner();
        let use_script_cache = self.is_eval_cache_active();
        async move {
            let args_val = args.try_into().map_err(|e| anyhow::anyhow!("{:?}", Into::<Error>::into(e)))?;
            Self::eval_async_inner(pool, use_replica, slot, command, script, lua_keys, args_val.into_array(), use_script_cache).await
        }
    }
}
