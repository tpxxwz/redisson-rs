use super::publish_subscribe_service::PublishSubscribeService;
use super::redisson_lock_entry::RedissonLockEntry;
use anyhow::Result;
use std::sync::Arc;

// ============================================================
// LockPubSub — 对应 Java org.redisson.pubsub.LockPubSub
// ============================================================

/// 锁专用 Pub/Sub，持有 PublishSubscribeService 并实现锁消息的分发逻辑。
/// 对应 Java LockPubSub extends PublishSubscribeService<RedissonLockEntry>。
pub struct LockPubSub {
    pub_sub_service: Arc<PublishSubscribeService>,
}

impl LockPubSub {
    /// 对应 Java LockPubSub.UNLOCK_MESSAGE
    pub const UNLOCK_MESSAGE: i64 = 0;
    /// 对应 Java LockPubSub.READ_UNLOCK_MESSAGE
    pub const READ_UNLOCK_MESSAGE: i64 = 1;

    pub fn new(pub_sub_service: Arc<PublishSubscribeService>) -> Self {
        Self { pub_sub_service }
    }

    /// 对应 Java PublishSubscribeService.subscribe()
    pub async fn subscribe(
        &self,
        entry_name: &str,
        channel_name: &str,
    ) -> Result<Arc<RedissonLockEntry>> {
        self.pub_sub_service
            .subscribe(entry_name, channel_name)
            .await
    }

    /// 对应 Java PublishSubscribeService.unsubscribe()
    pub async fn unsubscribe(&self, entry_name: &str, channel_name: &str) -> Result<()> {
        self.pub_sub_service
            .unsubscribe(entry_name, channel_name)
            .await
    }

    /// 对应 Java LockPubSub.onMessage(RedissonLockEntry entry, Long message)
    pub fn on_message(&self, channel: &str, message: Option<i64>) {
        let Some(entry_names) = self.pub_sub_service.channel_to_entries.get(channel) else {
            return;
        };
        for entry_name in entry_names.iter() {
            if let Some(entry) = self.pub_sub_service.entries.get(&*entry_name) {
                match message {
                    Some(Self::READ_UNLOCK_MESSAGE) => entry.try_run_all_listeners(),
                    Some(Self::UNLOCK_MESSAGE) | _ => entry.try_run_listener(),
                }
                tracing::debug!(
                    "Lock release notification: channel={}, entry={}, msg={:?}",
                    channel,
                    *entry_name,
                    message,
                );
            }
        }
    }
}
