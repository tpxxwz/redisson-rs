use crate::client::message_listener::MessageListener;
use crate::client::pattern_message_listener::PatternMessageListener;
use crate::client::protocol::pubsub::pubsub_type::PubSubType;
use crate::client::redis_pubsub_listener::RedisPubSubListener;
use crate::command::ListenerMessage;
use std::sync::Arc;

pub struct PubSubPatternMessageListener {
    listener: Arc<dyn PatternMessageListener>,
    name: String,
}

impl PubSubPatternMessageListener {
    pub fn new(listener: Arc<dyn PatternMessageListener>, name: String) -> Self {
        Self { listener, name }
    }
}

impl MessageListener for PubSubPatternMessageListener {
    fn on_message(&self, _channel: &str, _msg: ListenerMessage) {
        // default empty implementation
    }
}

impl RedisPubSubListener for PubSubPatternMessageListener {
    fn on_status(&self, _type: PubSubType, _channel: &str) {
        // default empty implementation
    }

    fn on_pattern_message(&self, pattern: &str, channel: &str, message: ListenerMessage) {
        if self.name == pattern {
            self.listener.on_message(pattern, channel, message);
        }
    }
}
