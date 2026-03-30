use std::sync::Arc;

// ============================================================
// NameMapper — 对应 Java org.redisson.config.NameMapper
// ============================================================

/// 对应 Java NameMapper 接口
pub trait NameMapper: Send + Sync {
    /// 对应 Java NameMapper.map(String name)
    fn map(&self, name: &str) -> String;

    /// 对应 Java NameMapper.unmap(String name)
    fn unmap(&self, name: &str) -> String;
}

// ============================================================
// DefaultNameMapper — 对应 Java DefaultNameMapper（直接透传）
// ============================================================

pub struct DefaultNameMapper;

impl NameMapper for DefaultNameMapper {
    fn map(&self, name: &str) -> String {
        name.to_string()
    }

    fn unmap(&self, name: &str) -> String {
        name.to_string()
    }
}

/// 对应 Java NameMapper.direct()
pub fn direct() -> Arc<dyn NameMapper> {
    Arc::new(DefaultNameMapper)
}
