// ============================================================
// RedissonClientSideCaching — 对应 Java org.redisson.RedissonClientSideCaching
// ============================================================

pub trait RedissonClientSideCaching: Send + Sync {
    fn clear_cache(&self, _name: &str) {}
}
