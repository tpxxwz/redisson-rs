use crate::command::ListenerMessage;

pub trait PatternMessageListener: Send + Sync {
    fn on_message(&self, pattern: &str, channel: &str, msg: ListenerMessage);
}

impl<F> PatternMessageListener for F
where
    F: Fn(&str, &str, ListenerMessage) + Send + Sync,
{
    fn on_message(&self, pattern: &str, channel: &str, msg: ListenerMessage) {
        self(pattern, channel, msg)
    }
}
