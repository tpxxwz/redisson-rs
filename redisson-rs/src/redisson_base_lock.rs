// use fred::types::Value;
// use anyhow::Result;
// use std::future::Future;
// use std::ops::Deref;
// use std::sync::Arc;
// use crate::command::command_async_executor::CommandAsyncExecutor;
// use crate::ext::RedisKey;
// 
// // ============================================================
// // LockInner — 抽象方法 trait（子类实现）
// // 对应 Java RedissonBaseLock 中的 abstract 方法
// // ============================================================
// 
// /// 锁内部操作 trait，对应 Java RedissonBaseLock 中的抽象方法
// /// 子类（如 RedissonLock）需要实现这些方法
// pub trait LockInner: Send + Sync {
//     /// 对应 Java RedissonBaseLock.unlockInnerAsync(long threadId, String requestId, int timeout)
//     /// 返回 None 表示非当前线程持有，Some(0) 表示部分解锁，Some(1) 表示完全解锁
//     fn unlock_inner_async(
//         &self,
//         thread_id: &str,
//         request_id: &str,
//         timeout: i32,
//     ) -> impl Future<Output = Result<Option<i64>>> + Send;
// 
//     /// 对应 Java RedissonLock.forceUnlockAsync()
//     fn force_unlock_inner_async(&self) -> impl Future<Output = Result<bool>> + Send;
// 
//     /// 对应 Java RedissonBaseLock.cancelExpirationRenewal 的子类覆写版本
//     /// 在 RedissonLock 中会重置 internalLockLeaseTime
//     fn cancel_expiration_renewal(&self, thread_id: Option<&str>, unlock_result: Option<bool>);
// }
// 
// // ============================================================
// // RLockBase — 公共方法 trait（默认实现）
// // 对应 Java RedissonBaseLock 中 RLock 接口的公共实现
// // ============================================================
// 
// /// 锁公共方法 trait，提供默认实现
// /// 对应 Java RedissonBaseLock 中实现 RLock 接口的方法
// /// 使用泛型参数 CE 避免 dyn CommandAsyncExecutor 的兼容性问题
// pub trait RLockBase<CE: CommandAsyncExecutor>: LockInner {
//     // ── 访问器（子类实现）───────────────────────────────────────────
// 
//     /// 获取节点 ID
//     fn lock_id(&self) -> &str;
//     /// 获取 entry 名称
//     fn entry_name(&self) -> &str;
//     /// 获取命令执行器
//     fn command_executor(&self) -> &Arc<CE>;
//     /// 获取锁名称（Redis key）
//     fn lock_name(&self) -> String;
// 
//     // ── 辅助方法（有默认实现）─────────────────────────────────────────
// 
//     /// 生成锁名字（id:threadId）
//     fn generate_lock_name(&self, thread_id: &str) -> String {
//         format!("{}:{}", self.lock_id(), thread_id)
//     }
// 
//     /// 获取解锁锁存器名称
//     fn get_unlock_latch_name(&self, request_id: &str) -> String {
//         format!(
//             "{}:{}",
//             prefix_name("redisson_unlock_latch", &self.lock_name()),
//             request_id
//         )
//     }
// 
//     /// 计算解锁超时时间
//     fn calc_unlock_timeout(&self) -> i32 {
//         self.command_executor()
//             .service_manager()
//             .calc_unlock_latch_timeout_ms() as i32
//     }
// 
//     /// 调度过期续约
//     fn schedule_expiration_renewal(&self, thread_id: &str) {
//         self.command_executor()
//             .service_manager()
//             .renewal_scheduler()
//             .renew_lock(
//                 self.lock_name(),
//                 self.generate_lock_name(thread_id),
//                 thread_id.to_string(),
//             );
//     }
// 
//     /// 取消过期续约（基类版本）
//     fn cancel_expiration_renewal_base(&self, thread_id: Option<&str>) {
//         self.command_executor()
//             .service_manager()
//             .renewal_scheduler()
//             .cancel_lock_renewal(&self.lock_name(), thread_id);
//     }
// 
//     // ── 公共方法（RLock 接口，有默认实现）─────────────────────────────
// 
//     /// 对应 Java RedissonBaseLock.isLocked()
//     async fn is_locked(&self) -> Result<bool> {
//         use crate::client::protocol::redis_commands as commands;
//         let key = self.lock_name();
//         self.command_executor()
//             .read_async(&key, commands::EXISTS, vec![Value::from(key.clone())])
//             .await
//     }
// 
//     /// 对应 Java RedissonBaseLock.isHeldByThread(long threadId)
//     async fn is_held_by_thread(&self, thread_id: &str) -> Result<bool> {
//         use crate::client::protocol::redis_commands as commands;
//         let key = self.lock_name();
//         let lock_name = self.generate_lock_name(thread_id);
//         self.command_executor()
//             .read_async(
//                 &key,
//                 commands::HEXISTS,
//                 vec![Value::from(key.clone()), Value::from(lock_name)],
//             )
//             .await
//     }
// 
//     /// 对应 Java RedissonBaseLock.isHeldByCurrentThread()
//     async fn is_held_by_current_thread(&self, thread_id: &str) -> Result<bool> {
//         self.is_held_by_thread(thread_id).await
//     }
// 
//     /// 对应 Java RedissonBaseLock.remainTimeToLive()
//     async fn remain_time_to_live(&self) -> Result<i64> {
//         use crate::client::protocol::redis_commands as commands;
//         let key = self.lock_name();
//         self.command_executor()
//             .read_async(&key, commands::PTTL, vec![Value::from(key.clone())])
//             .await
//     }
// 
//     /// 对应 Java RedissonBaseLock.getHoldCount()
//     async fn get_hold_count(&self, thread_id: &str) -> Result<i64> {
//         let key = self.lock_name();
//         let lock_name = self.generate_lock_name(thread_id);
//         let script = "
//             if redis.call('hexists', KEYS[1], ARGV[1]) == 1 then
//                 return redis.call('hget', KEYS[1], ARGV[1])
//             else
//                 return 0
//             end
//         ";
//         self.command_executor()
//             .eval_write_async(key.as_str(), crate::client::protocol::redis_commands::EVAL_LONG, script, vec![key.as_str()], vec![Value::from(lock_name)])
//             .await
//     }
// 
//     /// 对应 Java RedissonBaseLock.unlockAsync(long threadId)
//     async fn unlock_async(&self, thread_id: &str) -> Result<()> {
//         let request_id = uuid::Uuid::new_v4().to_string();
//         let timeout = self.calc_unlock_timeout();
// 
//         let result = self
//             .unlock_inner_async(thread_id, &request_id, timeout)
//             .await?;
// 
//         self.cancel_expiration_renewal(Some(thread_id), result.map(|v| v == 1));
// 
//         if result.is_none() {
//             anyhow::bail!(
//                 "attempt to unlock lock, not locked by current thread by node id: {} thread-id: {}",
//                 self.lock_id(),
//                 thread_id
//             );
//         }
// 
//         Ok(())
//     }
// 
//     /// 对应 Java RedissonBaseLock.forceUnlock()
//     async fn force_unlock(&self) -> Result<bool> {
//         self.cancel_expiration_renewal(None, None);
//         self.force_unlock_inner_async().await
//     }
// 
//     /// 对应 Java RedissonBaseLock.deleteAsync() - 委托给 forceUnlockAsync
//     async fn delete_async(&self) -> Result<bool> {
//         self.force_unlock().await
//     }
// }
// 
// // ============================================================
// // RedissonBaseLock — 对应 Java abstract class RedissonBaseLock
// // extends RedissonExpirable implements RLock
// // ============================================================
// 
// /// 分布式锁公共基类，对应 Java abstract class RedissonBaseLock。
// /// 通过组合 `RedissonExpirable` 继承 `RedissonObject` 的功能。
// pub struct RedissonBaseLock<CE: CommandAsyncExecutor> {
//     // ── 来自 RedissonBaseLock 的字段 ─────────────────────────────────
//     /// 对应 Java RedissonBaseLock.id（节点 UUID）
//     pub(crate) id: String,
//     /// 对应 Java RedissonBaseLock.entryName = id + ":" + name
//     pub(crate) entry_name: String,
// }
// 
// impl<CE: CommandAsyncExecutor> RedissonBaseLock<CE> {
//     /// 对应 Java RedissonBaseLock(CommandAsyncExecutor commandExecutor, String name)
//     pub fn new(command_executor: &Arc<CE>, name: impl RedisKey) -> Self {
//         let service_manager = command_executor.service_manager();
//         let id = service_manager.id();
//         let name_str = name.key();
//         let entry_name = format!("{}:{}", id, name_str);
// 
//         Self {
//             id: id.to_string(),
//             entry_name,
//         }
//     }
// }
// 
// // ============================================================
// // impl LockInner for RedissonBaseLock
// // 占位符实现，子类应提供真正的实现
// // ============================================================
// 
// impl<CE: CommandAsyncExecutor> LockInner for RedissonBaseLock<CE> {
//     /// 占位符实现，子类应覆盖
//     async fn unlock_inner_async(
//         &self,
//         _thread_id: &str,
//         _request_id: &str,
//         _timeout: i32,
//     ) -> Result<Option<i64>> {
//         anyhow::bail!("unlock_inner_async must be implemented by subclass");
//     }
// 
//     /// 占位符实现，子类应覆盖
//     async fn force_unlock_inner_async(&self) -> Result<bool> {
//         anyhow::bail!("force_unlock_inner_async must be implemented by subclass");
//     }
// 
//     /// 占位符实现，子类应覆盖
//     fn cancel_expiration_renewal(&self, _thread_id: Option<&str>, _unlock_result: Option<bool>) {
//         // 基类默认实现：什么都不做
//         // 子类（如 RedissonLock）会覆盖此方法
//     }
// }
