use super::redisson_lock_entry::RedissonLockEntry;
use crate::client::channel_name::ChannelName;
use crate::client::protocol::pubsub::pubsub_type::SubscribeType;
use crate::client::redis_pubsub_listener::{MultipleRedisPubSubListeners, RedisPubSubListener};
use crate::config::{RedissonConfig, ServerMode, build_connection_config, build_fred_config, build_perf_config};
use crate::connection::connection_manager::ConnectionManager;
use anyhow::{Context, Result};
use dashmap::{DashMap, DashSet};
use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, EventInterface, PubsubInterface};
use fred::prelude::ReconnectPolicy;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
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
    semaphores: Vec<Arc<Semaphore>>,
    pub(crate) entries: DashMap<String, Arc<RedissonLockEntry>>,
    pub(crate) channel_to_entries: DashMap<String, DashSet<String>>,
    /// pattern → listeners 映射，用于消息到来时回调分发
    pattern_listeners: DashMap<ChannelName, Vec<Arc<dyn RedisPubSubListener>>>,
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
    pub async fn new(config: &RedissonConfig, publish_command: &'static str) -> Result<Self> {
        let semaphores = (0..50).map(|_| Arc::new(Semaphore::new(1))).collect();

        let reconnect_policy = ReconnectPolicy::new_exponential(
            config.reconnect_max_attempts,
            config.reconnect_min_delay_ms,
            config.reconnect_max_delay_ms,
            config.reconnect_multiplier,
        );

        let subscriber = SubscriberClient::new(
            build_fred_config(config)?,
            Some(build_perf_config(config)),
            Some(build_connection_config(config)),
            Some(reconnect_policy),
        );

        // manage_subscriptions 内部自己 spawn task，负责普通 channel/pattern 的断线重订阅
        subscriber.manage_subscriptions();

        if matches!(config.mode, ServerMode::Cluster) {
            let subscriber = subscriber.clone();
            tokio::spawn(async move {
                let mut reconnect_rx = subscriber.reconnect_rx();
                while let Ok(server) = reconnect_rx.recv().await {
                    let subscriber = subscriber.clone();
                    tokio::spawn(async move {
                        let patterns: Vec<String> = subscriber
                            .tracked_patterns()
                            .into_iter()
                            .map(|p| p.to_string())
                            .filter(|p| ChannelName::from(p.clone()).is_keyspace())
                            .collect();
                        if !patterns.is_empty() {
                            if let Err(e) = subscriber
                                .to_client()
                                .with_cluster_node(&server)
                                .psubscribe(patterns)
                                .await
                            {
                                tracing::warn!("psubscribe keyspace patterns to {} failed: {}", server, e);
                            }
                        }
                    });
                }
            });
        }

        subscriber
            .init()
            .await
            .context("Failed to connect subscriber")?;

        tracing::info!("Redis Pub/Sub subscriber established");

        Ok(Self {
            subscriber,
            connection_manager: OnceLock::new(),
            semaphores,
            entries: DashMap::new(),
            channel_to_entries: DashMap::new(),
            pattern_listeners: DashMap::new(),
            publish_command,
        })
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

    /// 对应 Java PublishSubscribeService.getPublishCommand()
    pub fn publish_command(&self) -> &'static str {
        self.publish_command
    }

    /// 对应 Java PublishSubscribeService.psubscribe()
    pub async fn psubscribe(
        &self,
        channel_name: ChannelName,
        listeners: impl Into<MultipleRedisPubSubListeners>,
    ) -> Result<()> {
        let listeners = listeners.into().into_vec();

        let already = self.pattern_listeners.contains_key(&channel_name);
        self.pattern_listeners
            .entry(channel_name.clone())
            .or_default()
            .extend(listeners);

        if already {
            return Ok(());
        }

        let is_multi_entity = channel_name.is_keyspace()
            && matches!(self.connection_manager()?.config().mode, ServerMode::Cluster);

        let result = if is_multi_entity {
            let subscriber = &self.subscriber;
            let mut set = tokio::task::JoinSet::new();
            for server in subscriber.active_connections() {
                let subscriber = subscriber.clone();
                let ch = channel_name.to_string();
                set.spawn(async move {
                    subscriber
                        .to_client()
                        .with_cluster_node(&server)
                        .psubscribe(ch.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("psubscribe to {} on {} failed: {}", ch, server, e))
                });
            }
            let mut first_err = None;
            while let Some(res) = set.join_next().await {
                if let Err(e) = res.map_err(|e| anyhow::anyhow!("task panicked: {}", e)).and_then(|r| r) {
                    first_err.get_or_insert(e);
                }
            }
            first_err.map_or(Ok(()), Err)
        } else {
            self.subscribe_internal(SubscribeType::Psubscribe, channel_name.to_string()).await
        };

        if result.is_err() {
            if let Some(mut entry) = self.pattern_listeners.get_mut(&channel_name) {
                entry.clear();
            }
        }
        result
    }

    pub async fn subscribe(
        &self,
        entry_name: &str,
        channel_name: &str,
    ) -> Result<Arc<RedissonLockEntry>> {
        let entry = self
            .entries
            .entry(entry_name.to_string())
            .or_insert_with(|| Arc::new(RedissonLockEntry::new()))
            .clone();

        let need_subscribe = entry.is_empty();
        entry.add_waiter(task::id());

        if need_subscribe {
            let result = self
                .subscribe_internal(SubscribeType::Subscribe, channel_name.to_string())
                .await;
            if result.is_err() {
                entry.remove_waiter();
                return Err(result.unwrap_err());
            }
        }

        self.channel_to_entries
            .entry(channel_name.to_string())
            .or_insert_with(DashSet::new)
            .insert(entry_name.to_string());

        Ok(entry)
    }

    pub async fn unsubscribe(&self, entry_name: &str, channel_name: &str) -> Result<()> {
        let entry = self
            .entries
            .entry(entry_name.to_string())
            .or_insert_with(|| Arc::new(RedissonLockEntry::new()))
            .clone();

        let semaphore = self.get_semaphore(channel_name);
        let permit = semaphore
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to acquire semaphore: {}", e))?;

        entry.remove_waiter();
        let waiters_count = entry.waiters_count();

        if waiters_count == 0 {
            let _ = self
                .subscriber()
                .unsubscribe(channel_name.to_string())
                .await;
            entry.subscribe_count.store(0, Ordering::Relaxed);
            self.entries.remove(entry_name);

            if let Some((_, entry_names)) = self.channel_to_entries.remove(channel_name) {
                entry_names.remove(entry_name);
                if !entry_names.is_empty() {
                    self.channel_to_entries
                        .insert(channel_name.to_string(), entry_names);
                }
            }

            tracing::debug!(
                "Unsubscribed from Redis channel: {} (entry: {})",
                channel_name,
                entry_name
            );
        } else {
            tracing::trace!(
                "Removed waiter from channel: {} (remaining: {})",
                channel_name,
                waiters_count
            );
        }

        drop(permit);
        Ok(())
    }

    pub fn stats(&self) -> PublishSubscribeStats {
        let total_entries = self.entries.len();
        let total_waiters = self
            .entries
            .iter()
            .map(|entry| entry.value().waiters_count())
            .sum();
        let total_channels = self.channel_to_entries.len();

        PublishSubscribeStats {
            total_entries,
            total_waiters,
            total_channels,
            semaphore_shards: 50,
        }
    }

    /// 对应 Java PublishSubscribeService.subscribe(PubSubType, ...)
    /// 统一入口：根据 SubscribeType 调对应的 fred 方法，不做路由判断。
    async fn subscribe_internal(&self, sub_type: SubscribeType, channel_name: String) -> Result<()> {
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

    fn get_semaphore(&self, channel_name: &str) -> Arc<Semaphore> {
        let hash = self.channel_hash(channel_name);
        self.semaphores[hash as usize % 50].clone()
    }

    fn channel_hash(&self, channel_name: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        channel_name.hash(&mut hasher);
        hasher.finish()
    }
}
