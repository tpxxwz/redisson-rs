use crate::client::channel_name::ChannelName;
use crate::client::pattern_message_listener::PatternMessageListener;
use crate::client::protocol::pubsub::pubsub_type::{PubSubType, UnsubscribeType};
use crate::client::redis_pubsub_listener::RedisPubSubListener;
use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::connection::connection_manager::ConnectionManager;
use crate::pubsub::publish_subscribe_service::PublishSubscribeService;
use crate::pubsub_pattern_message_listener::PubSubPatternMessageListener;
use anyhow::Result;
use std::sync::Arc;

pub trait RPatternTopic: Send + Sync {
    fn pattern_topic_inner(&self) -> &RedissonPatternTopicInner;

    fn add_listener<T: Send + Sync + 'static>(
        &self,
        listener: Arc<dyn PatternMessageListener>,
    ) -> Result<usize> {
        let inner = self.pattern_topic_inner();
        let pubsub_listener = Arc::new(PubSubPatternMessageListener::new(
            listener,
            inner.name.clone(),
        ));
        inner.add_pubsub_listener(pubsub_listener)
    }

    fn remove_listener(&self, ids: &[usize]) {
        let inner = self.pattern_topic_inner();
        inner
            .subscribe_service
            .remove_listener(PubSubType::Unsubscribe(UnsubscribeType::Punsubscribe), inner.channel_name)
    }
}

struct RedissonPatternTopicInner {
    pub(crate) subscribe_service: Arc<PublishSubscribeService>,
    pub(crate) command_executor: Arc<dyn CommandAsyncExecutor>,
    pub(crate) name: String,
    pub(crate) channel_name: ChannelName,
}

impl RedissonPatternTopicInner {
    pub fn new(command_executor: Arc<dyn CommandAsyncExecutor>, name: String) -> Self {
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

    pub fn add_pubsub_listener(
        &self,
        pubsub_listener: Arc<dyn RedisPubSubListener>,
    ) -> Result<usize> {
        let id = Arc::as_ptr(&pubsub_listener) as *const () as usize;
        self.subscribe_service
            .psubscribe(self.channel_name, pubsub_listener)?;
        Ok(id)
    }
}
