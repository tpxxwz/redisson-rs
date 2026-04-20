use crate::client::channel_name::ChannelName;
use crate::client::listener_id::{ListenerId, MultipleListenerIds};
use crate::client::pattern_message_listener::PatternMessageListener;
use crate::client::protocol::pubsub::pubsub_type::UnsubscribeType;
use crate::client::redis_pubsub_listener::RedisPubSubListener;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use crate::pubsub_pattern_message_listener::PubSubPatternMessageListener;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use crate::command::command_async_service::CommandAsyncService;

#[async_trait]
pub trait RPatternTopic: Send + Sync {
    fn pattern_topic_inner(&self) -> &RedissonPatternTopicInner;

    async fn add_listener(
        &self,
        listener: Arc<dyn PatternMessageListener>,
    ) -> Result<ListenerId> {
        let inner = self.pattern_topic_inner();
        let pubsub_listener = Arc::new(PubSubPatternMessageListener::new(
            listener,
            inner.name.clone(),
        ));
        inner.add_pubsub_listener(pubsub_listener).await
    }

    async fn remove_listener(&self, ids: impl Into<MultipleListenerIds> + Send) -> Result<()> {
        let inner = self.pattern_topic_inner();
        inner
            .subscribe_service
            .remove_listener(UnsubscribeType::Punsubscribe, inner.channel_name.clone(), ids)
            .await
    }
}

/// 对应 Java org.redisson.RedissonPatternTopic
pub struct RedissonPatternTopic {
    inner: RedissonPatternTopicInner,
}

impl RedissonPatternTopic {
    pub fn new(command_executor: Arc<CommandAsyncService>, pattern: String) -> Self {
        Self {
            inner: RedissonPatternTopicInner::new(command_executor, pattern),
        }
    }
}

impl RPatternTopic for RedissonPatternTopic {
    fn pattern_topic_inner(&self) -> &RedissonPatternTopicInner {
        &self.inner
    }
}

struct RedissonPatternTopicInner {
    pub(crate) subscribe_service: Arc<PublishSubscribeService>,
    pub(crate) command_executor: Arc<CommandAsyncService>,
    pub(crate) name: String,
    pub(crate) channel_name: ChannelName,
}

impl RedissonPatternTopicInner {
    pub fn new(command_executor: Arc<CommandAsyncService>, name: String) -> Self {
        let channel_name = ChannelName::from(name.as_str());
        let subscribe_service = command_executor
            .connection_manager()
            .subscribe_service()
            .clone();
        Self {
            subscribe_service,
            command_executor,
            name,
            channel_name,
        }
    }

    pub async fn add_pubsub_listener(
        &self,
        pubsub_listener: Arc<dyn RedisPubSubListener>,
    ) -> Result<ListenerId> {
        let id = ListenerId::from(Arc::as_ptr(&pubsub_listener) as *const () as usize);
        self.subscribe_service
            .psubscribe(self.channel_name.clone(), pubsub_listener)
            .await?;
        Ok(id)
    }
}
