use crate::client::channel_name::{ChannelName, MultipleChannelNames};
use crate::client::listener_id::{ListenerId, MultipleListenerIds};
use crate::client::protocol::pubsub::pubsub_type::{SubscribeType, UnsubscribeType};
use crate::client::redis_pubsub_listener::{MultipleRedisPubSubListeners, RedisPubSubListener};
use crate::config::{
    RedissonConfig, ServerMode, build_connection_config, build_fred_config, build_perf_config,
};
use crate::connection::connection_manager::ConnectionManager;
use anyhow::{Context, Result};
use dashmap::DashMap;
use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, ClusterInterface, EventInterface, PubsubInterface};
use fred::types::Value;
use fred::prelude::{ReconnectPolicy, Server};
use fred::types::MessageKind;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::Semaphore;
use tokio::task;

/// Pub/Sub 订阅服务，管理 channel → entry 的映射和订阅生命周期。
/// 对应 Java PublishSubscribeService，通过 MasterSlaveConnectionManager.subscribeService 持有。
pub struct PublishSubscribeService {
    /// Pub/Sub 专用订阅客户端
    subscriber: SubscriberClient,
    /// 持有 ConnectionManager 的弱引用，避免循环 Arc 引用
    /// 用 OnceLock 延迟注入，因为 ConnectionManager 在 PublishSubscribeService 之后才构建完成
    connection_manager: OnceLock<Weak<dyn ConnectionManager>>,
    config: Arc<RedissonConfig>,
    semaphores: Vec<Arc<Semaphore>>,
    /// 普通 PSUBSCRIBE pattern → listeners（非 keyspace）
    pub(crate) pattern_listeners: DashMap<ChannelName, Vec<Arc<dyn RedisPubSubListener>>>,
    /// 集群 keyspace PSUBSCRIBE pattern → listeners（断线重连时需按节点重订阅）
    keyspace_pattern_listeners: DashMap<ChannelName, Vec<Arc<dyn RedisPubSubListener>>>,
    /// SUBSCRIBE channel → listeners
    channel_listeners: DashMap<ChannelName, Vec<Arc<dyn RedisPubSubListener>>>,
    /// SSUBSCRIBE sharded channel → listeners
    shard_channel_listeners: DashMap<ChannelName, Vec<Arc<dyn RedisPubSubListener>>>,
    /// 对应 Java PublishSubscribeService.getPublishCommand()
    /// standalone/sentinel 用 "publish"，cluster sharded 用 "spublish"
    publish_command: &'static str,
}

#[derive(Debug, Clone)]
pub struct PublishSubscribeStats {
    pub total_entries: usize,
    pub total_waiters: usize,
    pub total_channels: usize,
    pub semaphore_shards: usize,
}

impl PublishSubscribeService {
    /// 对应 Java MasterSlaveConnectionManager 构造器里对 subscribeService 调用的初始化逻辑。
    /// 创建并连接 SubscriberClient，启动自动重订阅任务。
    pub async fn new(
        config: Arc<RedissonConfig>,
        publish_command: &'static str,
    ) -> Result<Arc<Self>> {
        let semaphores = (0..50).map(|_| Arc::new(Semaphore::new(1))).collect();

        let reconnect_policy = ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        );

        let subscriber = SubscriberClient::new(
            build_fred_config(&config)?,
            Some(build_perf_config(&config)),
            Some(build_connection_config(&config)),
            Some(reconnect_policy),
        );

        subscriber
            .init()
            .await
            .context("Failed to connect subscriber")?;

        tracing::info!("Redis Pub/Sub subscriber established");

        let svc = Arc::new(Self {
            subscriber,
            connection_manager: OnceLock::new(),
            config,
            semaphores,
            pattern_listeners: DashMap::new(),
            keyspace_pattern_listeners: DashMap::new(),
            channel_listeners: DashMap::new(),
            shard_channel_listeners: DashMap::new(),
            publish_command,
        });

        // 消息分发循环
        let dispatch_svc = svc.clone();
        tokio::spawn(async move {
            let mut rx = dispatch_svc.subscriber.message_rx();
            loop {
                let msg = match rx.recv().await {
                    Ok(m) => m,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("message_rx lagged, skipped {} events", n);
                        continue;
                    }
                    Err(_) => break,
                };
                match msg.kind {
                    MessageKind::PMessage => {
                        let channel = &*msg.channel;
                        let lm = msg.value.clone();
                        for entry in dispatch_svc.pattern_listeners.iter() {
                            let pattern: &str = entry.key();
                            if Self::redis_glob_matches(pattern, channel) {
                                for listener in entry.value().iter() {
                                    listener.on_pattern_message(pattern, channel, lm.clone());
                                }
                            }
                        }
                    }
                    MessageKind::Message => {
                        let channel = &*msg.channel;
                        let lm = msg.value.clone();

                        if let Some(listeners) = dispatch_svc
                            .channel_listeners
                            .get(&ChannelName::from(channel))
                        {
                            for listener in listeners.iter() {
                                listener.on_message(channel, lm.clone());
                            }
                        }
                    }
                    MessageKind::SMessage => {
                        let channel = &*msg.channel;
                        let lm = msg.value.clone();
                        if let Some(listeners) = dispatch_svc
                            .shard_channel_listeners
                            .get(&ChannelName::from(channel))
                        {
                            for listener in listeners.iter() {
                                listener.on_message(channel, lm.clone());
                            }
                        }
                    }
                }
            }
            tracing::warn!("pubsub message_rx closed");
        });

        // keyspace 事件分发循环（fred 不走 message_rx，需单独监听）
        let keyspace_svc = svc.clone();
        tokio::spawn(async move {
            let mut rx = keyspace_svc.subscriber.keyspace_event_rx();
            loop {
                let event = match rx.recv().await {
                    Ok(e) => e,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("keyspace_event_rx lagged, skipped {} events", n);
                        continue;
                    }
                    Err(_) => break,
                };
                eprintln!("[keyspace_rx] db={} op={} key={:?}", event.db, event.operation, String::from_utf8_lossy(event.key.as_bytes()));
                for entry in keyspace_svc.keyspace_pattern_listeners.iter() {
                    let pattern: &str = entry.key();
                    let key_str = String::from_utf8_lossy(event.key.as_bytes());
                    let (channel, lm): (String, Value) = if pattern.starts_with("__keyevent") {
                        (
                            format!("__keyevent@{}__:{}", event.db, event.operation),
                            Value::String(key_str.as_ref().into()),
                        )
                    } else {
                        (
                            format!("__keyspace@{}__:{}", event.db, key_str),
                            Value::String(event.operation.as_str().into()),
                        )
                    };
                    if Self::redis_glob_matches(pattern, &channel) {
                        for listener in entry.value().iter() {
                            listener.on_pattern_message(pattern, &channel, lm.clone());
                        }
                    }
                }
            }
            tracing::warn!("pubsub keyspace_event_rx closed");
        });

        // fred 内建的订阅管理任务，负责普通 channel/pattern/sharded channel 的断线重订阅
        svc.subscriber.manage_subscriptions();

        // cluster keyspace pattern 断线重订阅：用 keyspace_pattern_listeners 的 key 列表，
        // 避免依赖 tracked_patterns()（后者不含 with_cluster_node 路径的订阅）
        if matches!(svc.config.mode, ServerMode::Cluster { .. }) {
            let reconnect_svc = svc.clone();
            tokio::spawn(async move {
                let mut reconnect_rx = reconnect_svc.subscriber.reconnect_rx();
                while let Ok(server) = reconnect_rx.recv().await {
                    let subscriber = reconnect_svc.subscriber.clone();
                    let reconnect_svc = reconnect_svc.clone();
                    tokio::spawn(async move {
                        let patterns: Vec<String> = reconnect_svc
                            .keyspace_pattern_listeners
                            .iter()
                            .map(|e| e.key().to_string())
                            .collect();
                        if !patterns.is_empty() {
                            if let Err(e) = subscriber
                                .to_client()
                                .with_cluster_node(&server)
                                .psubscribe(patterns)
                                .await
                            {
                                tracing::warn!(
                                    "psubscribe keyspace patterns to {} failed: {}",
                                    server,
                                    e
                                );
                            }
                        }
                    });
                }
            });
        }

        Ok(svc)
    }

    pub fn set_connection_manager(&self, cm: Weak<dyn ConnectionManager>) {
        self.connection_manager
            .set(cm)
            .expect("connection_manager already set");
    }

    pub fn connection_manager(&self) -> Result<Arc<dyn ConnectionManager>> {
        self.connection_manager
            .get()
            .and_then(|w| w.upgrade())
            .ok_or_else(|| anyhow::anyhow!("connection_manager not set or dropped"))
    }

    // 对应 Java PublishSubscribeService.getPublishCommand()
    // pub fn publish_command(&self) -> &'static str {
    //     self.publish_command
    // }

    /// 对应 Java PublishSubscribeService.psubscribe()
    pub async fn psubscribe(
        &self,
        channel_name: ChannelName,
        listeners: impl Into<MultipleRedisPubSubListeners>,
    ) -> Result<()> {
        let listeners = listeners.into().into_vec();
        let is_keyspace = channel_name.is_keyspace();

        // keyspace channel（无论 standalone/cluster）统一存 keyspace_pattern_listeners
        let listener_map = if is_keyspace {
            &self.keyspace_pattern_listeners
        } else {
            &self.pattern_listeners
        };

        let already = listener_map.contains_key(&channel_name);
        listener_map
            .entry(channel_name.clone())
            .or_default()
            .extend(listeners);

        if already {
            return Ok(());
        }

        let result = if matches!(self.config.mode, ServerMode::Cluster { .. }) && is_keyspace {
            // cluster keyspace：向每个节点单独发 psubscribe（tracked_patterns 无法追踪这种方式）
            let servers = self.cluster_subscription_servers()?;
            eprintln!("[psubscribe] cluster keyspace pattern={} servers={:?}", channel_name, servers);
            let subscriber = &self.subscriber;
            let mut set = task::JoinSet::new();
            for server in servers {
                let subscriber = subscriber.clone();
                let ch = channel_name.clone();
                set.spawn(async move {
                    eprintln!("[psubscribe] sending to server={}", server);
                    let r = subscriber
                        .to_client()
                        .with_cluster_node(&server)
                        .psubscribe(ch.clone())
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!("psubscribe to {} on {} failed: {}", ch, server, e)
                        });
                    eprintln!("[psubscribe] server={} result={:?}", server, r.as_ref().map(|_| "ok").map_err(|e| e.to_string()));
                    r
                });
            }
            Self::join_all(&mut set).await
        } else {
            self.subscribe_internal(SubscribeType::Psubscribe, &channel_name)
                .await
        };

        if result.is_err() {
            if let Some(mut entry) = listener_map.get_mut(&channel_name) {
                entry.clear();
            }
        }
        result
    }

    /// 对应 Java PublishSubscribeService.subscribe(PubSubType, ...)
    /// 统一入口：根据 SubscribeType 调对应的 fred 方法，不做路由判断。
    async fn subscribe_internal(
        &self,
        sub_type: SubscribeType,
        channel_name: &ChannelName,
    ) -> Result<()> {
        let subscriber = &self.subscriber;
        match sub_type {
            SubscribeType::Subscribe => subscriber
                .subscribe(channel_name.clone())
                .await
                .map_err(|e| anyhow::anyhow!("subscribe to {} failed: {}", channel_name, e)),
            SubscribeType::Psubscribe => subscriber
                .psubscribe(channel_name.clone())
                .await
                .map_err(|e| anyhow::anyhow!("psubscribe to {} failed: {}", channel_name, e)),
            SubscribeType::Ssubscribe => subscriber
                .ssubscribe(channel_name.clone())
                .await
                .map_err(|e| anyhow::anyhow!("ssubscribe to {} failed: {}", channel_name, e)),
        }
    }

    /// 对应 Java PublishSubscribeService.removeListenerAsync(type, channelNames, listenerIds)
    /// 根据 unsub_type 选对应的 listener map，按 id 移除，收集空掉的 channel 后批量发 unsubscribe。
    pub async fn remove_listener(
        &self,
        unsub_type: UnsubscribeType,
        channel_names: impl Into<MultipleChannelNames>,
        ids: impl Into<MultipleListenerIds>,
    ) -> Result<()> {
        let ids = ids.into();
        let mut normal_unsub: Vec<ChannelName> = Vec::new();
        let mut keyspace_unsub: Vec<ChannelName> = Vec::new();

        for channel_name in &channel_names.into().into_vec() {
            let listener_map = match unsub_type {
                UnsubscribeType::Punsubscribe if channel_name.is_keyspace() => {
                    &self.keyspace_pattern_listeners
                }
                UnsubscribeType::Punsubscribe => &self.pattern_listeners,
                UnsubscribeType::Unsubscribe => &self.channel_listeners,
                UnsubscribeType::Sunsubscribe => &self.shard_channel_listeners,
            };

            let empty = if let Some(mut listeners) = listener_map.get_mut(channel_name) {
                listeners.retain(|l| {
                    let ptr = ListenerId::from(Arc::as_ptr(l) as *const () as usize);
                    !ids.contains(&ptr)
                });
                listeners.is_empty()
            } else {
                false
            };

            if empty {
                listener_map.remove(channel_name);
                if unsub_type == UnsubscribeType::Punsubscribe
                    && matches!(self.config.mode, ServerMode::Cluster { .. })
                    && channel_name.is_keyspace()
                {
                    keyspace_unsub.push(channel_name.clone());
                } else {
                    normal_unsub.push(channel_name.clone());
                }
            }
        }

        if !normal_unsub.is_empty() {
            match unsub_type {
                UnsubscribeType::Punsubscribe => {
                    self.subscriber
                        .punsubscribe(normal_unsub)
                        .await
                        .map_err(|e| anyhow::anyhow!("punsubscribe failed: {}", e))?;
                }
                UnsubscribeType::Unsubscribe => {
                    self.subscriber
                        .unsubscribe(normal_unsub)
                        .await
                        .map_err(|e| anyhow::anyhow!("unsubscribe failed: {}", e))?;
                }
                UnsubscribeType::Sunsubscribe => {
                    self.subscriber
                        .sunsubscribe(normal_unsub)
                        .await
                        .map_err(|e| anyhow::anyhow!("sunsubscribe failed: {}", e))?;
                }
            }
        }

        if !keyspace_unsub.is_empty() {
            let mut set = task::JoinSet::new();
            for server in self.cluster_subscription_servers()? {
                let subscriber = self.subscriber.clone();
                let channels = keyspace_unsub.clone();
                set.spawn(async move {
                    subscriber
                        .to_client()
                        .with_cluster_node(&server)
                        .punsubscribe(channels)
                        .await
                        .map_err(|e| anyhow::anyhow!("punsubscribe on {} failed: {}", server, e))
                });
            }
            Self::join_all(&mut set).await?;
        }

        Ok(())
    }

    async fn join_all(set: &mut task::JoinSet<Result<()>>) -> Result<()> {
        let mut first_err = None;
        while let Some(res) = set.join_next().await {
            if let Err(e) = res
                .map_err(|e| anyhow::anyhow!("task panicked: {}", e))
                .and_then(|r| r)
            {
                first_err.get_or_insert(e);
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    fn cluster_subscription_servers(&self) -> Result<Vec<Server>> {
        self.subscriber
            .cached_cluster_state()
            .map(|state| state.unique_primary_nodes())
            .ok_or_else(|| anyhow::anyhow!("cluster state not initialized"))
    }

    /// Redis glob 匹配（对应 Java GlobPatternMatcher），支持 * 和 ?。
    fn redis_glob_matches(pattern: &str, text: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = text.chars().collect();
        Self::glob_inner(&p, 0, &t, 0)
    }

    fn glob_inner(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
        if pi == p.len() {
            return ti == t.len();
        }
        match p[pi] {
            '*' => {
                // * 匹配零或多个字符
                (ti..=t.len()).any(|i| Self::glob_inner(p, pi + 1, t, i))
            }
            '?' => ti < t.len() && Self::glob_inner(p, pi + 1, t, ti + 1),
            c => ti < t.len() && t[ti] == c && Self::glob_inner(p, pi + 1, t, ti + 1),
        }
    }

    fn get_semaphore(&self, channel_name: &ChannelName) -> Arc<Semaphore> {
        self.semaphores[channel_name.hash_u64() as usize % 50].clone()
    }

    // /// 对应 Java PublishSubscribeService.subscribe()（带 listener 回调版）
    // pub async fn subscribe_with_listeners(
    //     &self,
    //     channel_name: ChannelName,
    //     listeners: impl Into<MultipleRedisPubSubListeners>,
    // ) -> Result<()> {
    //     let listeners = listeners.into().into_vec();
    //
    //     let already = self.channel_listeners.contains_key(&channel_name);
    //     self.channel_listeners
    //         .entry(channel_name.clone())
    //         .or_default()
    //         .extend(listeners);
    //
    //     if already {
    //         return Ok(());
    //     }
    //
    //     let result = self
    //         .subscribe_internal(SubscribeType::Subscribe, &channel_name)
    //         .await;
    //
    //     if result.is_err() {
    //         if let Some(mut entry) = self.channel_listeners.get_mut(&channel_name) {
    //             entry.clear();
    //         }
    //     }
    //     result
    // }
    //
    // /// 对应 Java PublishSubscribeService.ssubscribe()（sharded channel）
    // pub async fn ssubscribe(
    //     &self,
    //     channel_name: ChannelName,
    //     listeners: impl Into<MultipleRedisPubSubListeners>,
    // ) -> Result<()> {
    //     let listeners = listeners.into().into_vec();
    //
    //     let already = self.shard_channel_listeners.contains_key(&channel_name);
    //     self.shard_channel_listeners
    //         .entry(channel_name.clone())
    //         .or_default()
    //         .extend(listeners);
    //
    //     if already {
    //         return Ok(());
    //     }
    //
    //     let result = self
    //         .subscribe_internal(SubscribeType::Ssubscribe, &channel_name)
    //         .await;
    //
    //     if result.is_err() {
    //         if let Some(mut entry) = self.shard_channel_listeners.get_mut(&channel_name) {
    //             entry.clear();
    //         }
    //     }
    //     result
    // }
    //
    // pub async fn subscribe(
    //     &self,
    //     entry_name: &str,
    //     channel_name: &str,
    // ) -> Result<Arc<RedissonLockEntry>> {
    //     let entry = self
    //         .entries
    //         .entry(entry_name.to_string())
    //         .or_insert_with(|| Arc::new(RedissonLockEntry::new()))
    //         .clone();
    //
    //     let need_subscribe = entry.is_empty();
    //     entry.add_waiter(task::id());
    //
    //     if need_subscribe {
    //         let result = self
    //             .subscribe_internal(SubscribeType::Subscribe, &channel_name)
    //             .await;
    //         if result.is_err() {
    //             entry.remove_waiter();
    //             return Err(result.unwrap_err());
    //         }
    //     }
    //
    //     self.channel_to_entries
    //         .entry(channel_name.to_string())
    //         .or_insert_with(DashSet::new)
    //         .insert(entry_name.to_string());
    //
    //     Ok(entry)
    // }
    //
    // pub async fn unsubscribe(&self, entry_name: &str, channel_name: &str) -> Result<()> {
    //     let entry = self
    //         .entries
    //         .entry(entry_name.to_string())
    //         .or_insert_with(|| Arc::new(RedissonLockEntry::new()))
    //         .clone();
    //
    //     let semaphore = self.get_semaphore(channel_name);
    //     let permit = semaphore
    //         .acquire()
    //         .await
    //         .map_err(|e| anyhow::anyhow!("Failed to acquire semaphore: {}", e))?;
    //
    //     entry.remove_waiter();
    //     let waiters_count = entry.waiters_count();
    //
    //     if waiters_count == 0 {
    //         let _ = self
    //             .subscriber()
    //             .unsubscribe(channel_name.to_string())
    //             .await;
    //         entry.subscribe_count.store(0, Ordering::Relaxed);
    //         self.entries.remove(entry_name);
    //
    //         if let Some((_, entry_names)) = self.channel_to_entries.remove(channel_name) {
    //             entry_names.remove(entry_name);
    //             if !entry_names.is_empty() {
    //                 self.channel_to_entries
    //                     .insert(channel_name.to_string(), entry_names);
    //             }
    //         }
    //
    //         tracing::debug!(
    //             "Unsubscribed from Redis channel: {} (entry: {})",
    //             channel_name,
    //             entry_name
    //         );
    //     } else {
    //         tracing::trace!(
    //             "Removed waiter from channel: {} (remaining: {})",
    //             channel_name,
    //             waiters_count
    //         );
    //     }
    //
    //     drop(permit);
    //     Ok(())
    // }
    //
    // pub fn stats(&self) -> PublishSubscribeStats {
    //     let total_entries = self.entries.len();
    //     let total_waiters = self
    //         .entries
    //         .iter()
    //         .map(|entry| entry.value().waiters_count())
    //         .sum();
    //     let total_channels = self.channel_to_entries.len();
    //
    //     PublishSubscribeStats {
    //         total_entries,
    //         total_waiters,
    //         total_channels,
    //         semaphore_shards: 50,
    //     }
    // }
}
