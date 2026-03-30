use super::redisson_lock_entry::RedissonLockEntry;
use anyhow::Result;
use dashmap::{DashMap, DashSet};
use fred::clients::SubscriberClient;
use fred::interfaces::PubsubInterface;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::Semaphore;
use tokio::task;

// ============================================================
// PublishSubscribeService — 对应 Java org.redisson.pubsub.PublishSubscribeService
// ============================================================

/// Pub/Sub 订阅服务，管理 channel → entry 的映射和订阅生命周期。
/// 对应 Java PublishSubscribeService，通过 MasterSlaveConnectionManager.subscribeService 持有。
pub struct PublishSubscribeService {
    subscriber: SubscriberClient,
    semaphores: Vec<Arc<Semaphore>>,
    pub(crate) entries: DashMap<String, Arc<RedissonLockEntry>>,
    pub(crate) channel_to_entries: DashMap<String, DashSet<String>>,
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
    pub fn new(subscriber: SubscriberClient, publish_command: &'static str) -> Self {
        let semaphores = (0..50).map(|_| Arc::new(Semaphore::new(1))).collect();
        Self {
            subscriber,
            semaphores,
            entries: DashMap::new(),
            channel_to_entries: DashMap::new(),
            publish_command,
        }
    }

    /// 对应 Java PublishSubscribeService.getPublishCommand()
    pub fn publish_command(&self) -> &'static str {
        self.publish_command
    }

    pub async fn subscribe(
        &self,
        entry_name: &str,
        channel_name: &str,
    ) -> Result<Arc<RedissonLockEntry>> {
        let semaphore = self.get_semaphore(channel_name);
        let permit = semaphore
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to acquire semaphore: {}", e))?;

        let entry = self
            .entries
            .entry(entry_name.to_string())
            .or_insert_with(|| Arc::new(RedissonLockEntry::new()))
            .clone();

        let need_subscribe = entry.is_empty();
        entry.add_waiter(task::id());

        if need_subscribe {
            match self.subscriber.subscribe(channel_name.to_string()).await {
                Ok(()) => {
                    entry.subscribe_count.fetch_add(1, Ordering::Relaxed);
                    self.channel_to_entries
                        .entry(channel_name.to_string())
                        .or_insert_with(|| DashSet::new())
                        .insert(entry_name.to_string());
                    tracing::debug!(
                        "Subscribed to Redis channel: {} (entry: {})",
                        channel_name,
                        entry_name
                    );
                }
                Err(e) => {
                    entry.remove_waiter();
                    drop(permit);
                    return Err(e.into());
                }
            }
        }

        drop(permit);
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
            let _ = self.subscriber.unsubscribe(channel_name.to_string()).await;
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
