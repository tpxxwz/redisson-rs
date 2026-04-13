use crate::command::ListenerMessage;

pub trait MessageListener: Send + Sync {
    fn on_message(&self, channel: &str, msg: ListenerMessage);
}
impl<F> MessageListener for F
where
    F: Fn(&str, ListenerMessage) + Send + Sync,
{
    fn on_message(&self, channel: &str, msg: ListenerMessage) {
        self(channel, msg)
    }
}