use anyhow::Result;
use fred::types::Value;

pub mod boolean_amount_replay_convertor;
pub mod boolean_not_null_replay_convertor;
pub mod boolean_null_safe_replay_convertor;
pub mod boolean_replay_convertor;
pub mod empty_convertor;

pub use boolean_amount_replay_convertor::BooleanAmountReplayConvertor;
pub use boolean_not_null_replay_convertor::BooleanNotNullReplayConvertor;
pub use boolean_null_safe_replay_convertor::BooleanNullSafeReplayConvertor;
pub use boolean_replay_convertor::BooleanReplayConvertor;
pub use empty_convertor::EmptyConvertor;

// ============================================================
// Convertor<R> trait — 对应 Java org.redisson.client.protocol.convertor.Convertor<R>
// ============================================================

pub trait Convertor<R>: Send + Sync {
    /// 对应 Java Convertor.convert(Object obj)
    fn convert(&self, v: Value) -> Result<R>;
}
