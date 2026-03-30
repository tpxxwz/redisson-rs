use crate::connection::service_manager::RedisURI;
use std::sync::Arc;

// ============================================================
// NatMapper — 对应 Java org.redisson.api.NatMapper
// ============================================================

pub trait NatMapper: Send + Sync {
    fn map(&self, uri: RedisURI) -> RedisURI;
}

#[derive(Default)]
pub struct DirectNatMapper;

impl NatMapper for DirectNatMapper {
    fn map(&self, uri: RedisURI) -> RedisURI {
        uri
    }
}

pub fn direct() -> Arc<dyn NatMapper> {
    Arc::new(DirectNatMapper)
}
