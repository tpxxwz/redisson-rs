use std::sync::Arc;

// ============================================================
// CommandMapper — 对应 Java org.redisson.config.CommandMapper
// ============================================================

pub trait CommandMapper: Send + Sync {
    fn map(&self, command: &str) -> String;
}

#[derive(Default)]
pub struct DefaultCommandMapper;

impl CommandMapper for DefaultCommandMapper {
    fn map(&self, command: &str) -> String {
        command.to_string()
    }
}

pub fn direct() -> Arc<dyn CommandMapper> {
    Arc::new(DefaultCommandMapper)
}
