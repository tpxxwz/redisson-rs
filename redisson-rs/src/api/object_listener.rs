// ============================================================
// ObjectListener — 对应 Java org.redisson.api.ObjectListener
// ============================================================

/// Redisson Object Event listener for Expired or Deleted event.
/// 对应 Java ObjectListener (marker interface)
///
/// 对应 Java:
/// ```java
/// public interface ObjectListener extends EventListener {
/// }
/// ```
pub trait ObjectListener: Send + Sync {}

// ============================================================
// ExpiredObjectListener — 对应 Java org.redisson.api.ExpiredObjectListener
// ============================================================

/// Redisson Object Event listener for expired event published by Redis.
/// Redis notify-keyspace-events setting should contain Ex letters
///
/// 对应 Java:
/// ```java
/// @FunctionalInterface
/// public interface ExpiredObjectListener extends ObjectListener {
///     void onExpired(String name);
/// }
/// ```
pub trait ExpiredObjectListener: ObjectListener {
    /// Invoked on expired event
    /// 对应 Java: void onExpired(String name)
    fn on_expired(&self, name: &str);
}

// ============================================================
// DeletedObjectListener — 对应 Java org.redisson.api.DeletedObjectListener
// ============================================================

/// Redisson Object Event listener for deleted event published by Redis.
/// Redis notify-keyspace-events setting should contain Eg letters
///
/// 对应 Java:
/// ```java
/// @FunctionalInterface
/// public interface DeletedObjectListener extends ObjectListener {
///     void onDeleted(String name);
/// }
/// ```
pub trait DeletedObjectListener: ObjectListener {
    /// Invoked on deleted event
    /// 对应 Java: void onDeleted(String name)
    fn on_deleted(&self, name: &str);
}
