use crate::api::listener::EventListener;
use fred::types::KeyspaceEvent;

// ============================================================
// ObjectListener — 对应 Java org.redisson.api.ObjectListener
// ============================================================

/// 对象监听器标记 trait
/// 对应 Java org.redisson.api.ObjectListener extends EventListener
pub trait ObjectListener: EventListener {}

// ============================================================
// 通用对象事件 - org.redisson.api.*
// ============================================================

/// 过期对象监听器
/// 对应 Java org.redisson.api.ExpiredObjectListener
pub trait ExpiredObjectListener: ObjectListener {
    fn on_expired(&self, event: KeyspaceEvent);
}

/// 删除对象监听器
/// 对应 Java org.redisson.api.DeletedObjectListener
pub trait DeletedObjectListener: ObjectListener {
    fn on_deleted(&self, event: KeyspaceEvent);
}

/// 新对象监听器
/// 对应 Java org.redisson.api.NewObjectListener
pub trait NewObjectListener: ObjectListener {
    fn on_new_object(&self, event: KeyspaceEvent);
}

// ============================================================
// List 事件 - org.redisson.api.listener.List*Listener
// ============================================================

/// List 添加监听器
/// 对应 Java org.redisson.api.listener.ListAddListener
pub trait ListAddListener: ObjectListener {
    fn on_list_add(&self, event: KeyspaceEvent);
}

/// List 插入监听器
/// 对应 Java org.redisson.api.listener.ListInsertListener
pub trait ListInsertListener: ObjectListener {
    fn on_list_insert(&self, event: KeyspaceEvent);
}

/// List 删除监听器
/// 对应 Java org.redisson.api.listener.ListRemoveListener
pub trait ListRemoveListener: ObjectListener {
    fn on_list_remove(&self, event: KeyspaceEvent);
}

/// List 设置监听器
/// 对应 Java org.redisson.api.listener.ListSetListener
pub trait ListSetListener: ObjectListener {
    fn on_list_set(&self, event: KeyspaceEvent);
}

/// List 裁剪监听器
/// 对应 Java org.redisson.api.listener.ListTrimListener
pub trait ListTrimListener: ObjectListener {
    fn on_list_trim(&self, event: KeyspaceEvent);
}

// ============================================================
// Set 事件 - org.redisson.api.listener.Set*Listener
// ============================================================

/// Set 添加监听器
/// 对应 Java org.redisson.api.listener.SetAddListener
pub trait SetAddListener: ObjectListener {
    fn on_set_add(&self, event: KeyspaceEvent);
}

/// Set 删除监听器
/// 对应 Java org.redisson.api.listener.SetRemoveListener
pub trait SetRemoveListener: ObjectListener {
    fn on_set_remove(&self, event: KeyspaceEvent);
}

/// Set 随机删除监听器
/// 对应 Java org.redisson.api.listener.SetRemoveRandomListener
pub trait SetRemoveRandomListener: ObjectListener {
    fn on_set_remove_random(&self, event: KeyspaceEvent);
}

/// Set 对象监听器
/// 对应 Java org.redisson.api.listener.SetObjectListener
pub trait SetObjectListener: ObjectListener {
    fn on_set_object(&self, event: KeyspaceEvent);
}

// ============================================================
// Map 事件 - org.redisson.api.listener.Map*Listener
// ============================================================

/// Map Put 监听器
/// 对应 Java org.redisson.api.listener.MapPutListener
pub trait MapPutListener: ObjectListener {
    fn on_map_put(&self, event: KeyspaceEvent);
}

/// Map 删除监听器
/// 对应 Java org.redisson.api.listener.MapRemoveListener
pub trait MapRemoveListener: ObjectListener {
    fn on_map_remove(&self, event: KeyspaceEvent);
}

/// Map 过期监听器
/// 对应 Java org.redisson.api.listener.MapExpiredListener
pub trait MapExpiredListener: ObjectListener {
    fn on_map_expired(&self, event: KeyspaceEvent);
}

// ============================================================
// ScoredSortedSet 事件 - org.redisson.api.listener.ScoredSortedSet*Listener
// ============================================================

/// ScoredSortedSet 添加监听器
/// 对应 Java org.redisson.api.listener.ScoredSortedSetAddListener
pub trait ScoredSortedSetAddListener: ObjectListener {
    fn on_scored_sorted_set_add(&self, event: KeyspaceEvent);
}

/// ScoredSortedSet 删除监听器
/// 对应 Java org.redisson.api.listener.ScoredSortedSetRemoveListener
pub trait ScoredSortedSetRemoveListener: ObjectListener {
    fn on_scored_sorted_set_remove(&self, event: KeyspaceEvent);
}

// ============================================================
// Stream 事件 - org.redisson.api.listener.Stream*Listener
// ============================================================

/// Stream 添加监听器
/// 对应 Java org.redisson.api.listener.StreamAddListener
pub trait StreamAddListener: ObjectListener {
    fn on_stream_add(&self, event: KeyspaceEvent);
}

/// Stream 删除监听器
/// 对应 Java org.redisson.api.listener.StreamRemoveListener
pub trait StreamRemoveListener: ObjectListener {
    fn on_stream_remove(&self, event: KeyspaceEvent);
}

/// Stream 裁剪监听器
/// 对应 Java org.redisson.api.listener.StreamTrimListener
pub trait StreamTrimListener: ObjectListener {
    fn on_stream_trim(&self, event: KeyspaceEvent);
}

/// Stream 创建消费者监听器
/// 对应 Java org.redisson.api.listener.StreamCreateConsumerListener
pub trait StreamCreateConsumerListener: ObjectListener {
    fn on_stream_create_consumer(&self, event: KeyspaceEvent);
}

/// Stream 创建组监听器
/// 对应 Java org.redisson.api.listener.StreamCreateGroupListener
pub trait StreamCreateGroupListener: ObjectListener {
    fn on_stream_create_group(&self, event: KeyspaceEvent);
}

/// Stream 删除消费者监听器
/// 对应 Java org.redisson.api.listener.StreamRemoveConsumerListener
pub trait StreamRemoveConsumerListener: ObjectListener {
    fn on_stream_remove_consumer(&self, event: KeyspaceEvent);
}

/// Stream 删除组监听器
/// 对应 Java org.redisson.api.listener.StreamRemoveGroupListener
pub trait StreamRemoveGroupListener: ObjectListener {
    fn on_stream_remove_group(&self, event: KeyspaceEvent);
}

// ============================================================
// 其他事件
// ============================================================

/// IncrBy 监听器
/// 对应 Java org.redisson.api.listener.IncrByListener
pub trait IncrByListener: ObjectListener {
    fn on_incr_by(&self, event: KeyspaceEvent);
}

/// Tracking 监听器
/// 对应 Java org.redisson.api.listener.TrackingListener
pub trait TrackingListener: ObjectListener {
    fn on_tracking(&self, event: KeyspaceEvent);
}

/// Flush 监听器
/// 对应 Java org.redisson.api.listener.FlushListener
pub trait FlushListener: ObjectListener {
    fn on_flush(&self, event: KeyspaceEvent);
}

// ============================================================
// LocalCache 事件
// ============================================================

/// LocalCache 失效监听器
/// 对应 Java org.redisson.api.LocalCacheInvalidateListener
pub trait LocalCacheInvalidateListener: ObjectListener {
    fn on_local_cache_invalidate(&self, event: KeyspaceEvent);
}

/// LocalCache 更新监听器
/// 对应 Java org.redisson.api.LocalCacheUpdateListener
pub trait LocalCacheUpdateListener: ObjectListener {
    fn on_local_cache_update(&self, event: KeyspaceEvent);
}
