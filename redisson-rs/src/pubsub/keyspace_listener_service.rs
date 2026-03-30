use crate::api::object_listener::ObjectListener;
use anyhow::Result;
use dashmap::DashMap;
use fred::clients::SubscriberClient;
use fred::interfaces::PubsubInterface;
use fred::types::KeyspaceEvent;
use std::sync::atomic::{AtomicI32, Ordering};
use tokio::sync::broadcast::{Receiver, Sender};

// ============================================================
// EventCallback — 事件回调类型
// ============================================================

/// 事件回调函数类型
pub type EventCallback = Box<dyn Fn(KeyspaceEvent) + Send + Sync>;

// ============================================================
// EventListenerService — 对应 Java PublishSubscribeService
// ============================================================

/// 事件监听器管理服务
/// TODO: 需要重构以适配新的 trait-based ObjectListener 设计
pub struct EventListenerService {
    subscriber: SubscriberClient,
    /// pattern -> listeners 列表
    listeners: DashMap<String, Vec<(i32, EventCallback)>>,
    /// 下一个 listener ID
    next_id: AtomicI32,
    /// 事件广播 sender
    event_tx: Sender<KeyspaceEvent>,
}

impl EventListenerService {
    pub fn new(subscriber: SubscriberClient) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1024);

        Self {
            subscriber,
            listeners: DashMap::new(),
            next_id: AtomicI32::new(1),
            event_tx,
        }
    }

    /// 添加监听器（使用回调函数）
    /// 对应 Java RedissonObject.addListener(ObjectListener)
    pub async fn add_listener(&self, pattern: &str, callback: EventCallback) -> Result<i32> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let is_first = {
            let mut entry = self.listeners.entry(pattern.to_string()).or_default();
            let first = entry.is_empty();
            entry.push((id, callback));
            first
        };

        if is_first {
            self.subscriber.psubscribe(pattern).await?;
            tracing::debug!("PSUBSCRIBE {}", pattern);
        }

        Ok(id)
    }

    /// 移除监听器
    /// 对应 Java RedissonObject.removeListener(int listenerId)
    pub async fn remove_listener(&self, pattern: &str, id: i32) -> Result<()> {
        let is_empty = {
            if let Some(mut entry) = self.listeners.get_mut(pattern) {
                entry.retain(|(lid, _)| *lid != id);
                entry.is_empty()
            } else {
                return Ok(());
            }
        };

        if is_empty {
            self.listeners.remove(pattern);
            self.subscriber.punsubscribe(pattern).await?;
            tracing::debug!("PUNSUBSCRIBE {}", pattern);
        }

        Ok(())
    }

    /// 获取事件接收器
    pub fn event_rx(&self) -> Receiver<KeyspaceEvent> {
        self.event_tx.subscribe()
    }

    /// 分发事件
    pub fn dispatch(&self, event: KeyspaceEvent) {
        let _ = self.event_tx.send(event.clone());

        for entry in self.listeners.iter() {
            if self.matches(entry.key(), &event) {
                for (_, callback) in entry.value() {
                    callback(event.clone());
                }
            }
        }
    }

    fn matches(&self, pattern: &str, event: &KeyspaceEvent) -> bool {
        if pattern.starts_with("__keyevent@*:") {
            let suffix = &pattern["__keyevent@*:".len()..];
            // 处理 (a|b|c) 格式
            if suffix.starts_with('(') && suffix.ends_with(')') {
                let ops: Vec<&str> = suffix[1..suffix.len()-1].split('|').collect();
                return ops.contains(&event.operation.as_str());
            }
            return event.operation == suffix;
        }
        false
    }

    pub fn stats(&self) -> Stats {
        let mut total = 0;
        for entry in self.listeners.iter() {
            total += entry.value().len();
        }
        Stats {
            patterns: self.listeners.len(),
            listeners: total,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub patterns: usize,
    pub listeners: usize,
}
