use std::sync::Arc;
use crate::client::message_listener::MessageListener;
use crate::client::protocol::pubsub::pubsub_type::PubSubType;
use crate::command::ListenerMessage;

pub trait RedisPubSubListener: MessageListener {
    fn on_status(&self, _type: PubSubType, _channel: &str) {
        // default empty implementation
    }

    fn on_pattern_message(&self, _pattern: &str, _channel: &str, _message: ListenerMessage) {
        // default empty implementation
    }
}

pub struct MultipleRedisPubSubListeners {
    listeners: Vec<Arc<dyn RedisPubSubListener>>,
}

impl From<Arc<dyn RedisPubSubListener>> for MultipleRedisPubSubListeners {
    fn from(l: Arc<dyn RedisPubSubListener>) -> Self {
        Self { listeners: vec![l] }
    }
}

impl From<Vec<Arc<dyn RedisPubSubListener>>> for MultipleRedisPubSubListeners {
    fn from(l: Vec<Arc<dyn RedisPubSubListener>>) -> Self {
        Self { listeners: l }
    }
}

impl MultipleRedisPubSubListeners {
    pub fn into_vec(self) -> Vec<Arc<dyn RedisPubSubListener>> {
        self.listeners
    }
}
