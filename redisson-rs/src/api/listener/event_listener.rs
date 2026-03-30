use fred::types::Message;

// ============================================================
// EventListener — 对应 Java org.redisson.api.listener.EventListener
// ============================================================

/// 事件监听器标记 trait
/// 对应 Java org.redisson.api.listener.EventListener
pub trait EventListener: Send + Sync {}

// ============================================================
// MessageListener — 对应 Java org.redisson.api.listener.MessageListener
// ============================================================

/// 消息监听器
/// 对应 Java org.redisson.api.listener.MessageListener<M>
pub trait MessageListener: EventListener {
    fn on_message(&self, msg: Message);
}

// ============================================================
// PatternMessageListener — 对应 Java org.redisson.api.listener.PatternMessageListener
// ============================================================

/// Pattern 消息监听器
/// 对应 Java org.redisson.api.listener.PatternMessageListener<M>
pub trait PatternMessageListener: EventListener {
    fn on_pattern_message(&self, msg: Message);
}

// ============================================================
// StatusListener — 对应 Java org.redisson.api.listener.StatusListener
// Note: Java 的 BaseStatusListener 在 Rust 中不需要单独创建
//       因为 trait 已提供默认空实现
// ============================================================

/// 状态监听器
/// 对应 Java org.redisson.api.listener.StatusListener
pub trait StatusListener: EventListener {
    fn on_subscribe(&self, _channel: String) {}
    fn on_unsubscribe(&self, _channel: String) {}
}

// ============================================================
// PatternStatusListener — 对应 Java org.redisson.api.listener.PatternStatusListener
// Note: Java 的 BasePatternStatusListener 在 Rust 中不需要单独创建
//       因为 trait 已提供默认空实现
// ============================================================

/// Pattern 状态监听器
/// 对应 Java org.redisson.api.listener.PatternStatusListener
pub trait PatternStatusListener: EventListener {
    fn on_psubscribe(&self, _pattern: String) {}
    fn on_punsubscribe(&self, _pattern: String) {}
}
