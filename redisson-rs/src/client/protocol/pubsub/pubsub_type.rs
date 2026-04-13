#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscribeType {
    Subscribe,
    Psubscribe,
    Ssubscribe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsubscribeType {
    Unsubscribe,
    Punsubscribe,
    Sunsubscribe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PubSubType {
    Subscribe(SubscribeType),
    Unsubscribe(UnsubscribeType),
}
