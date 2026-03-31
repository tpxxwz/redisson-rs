// ============================================================
// SlotCallback — 对应 Java org.redisson.SlotCallback（接口）
// ============================================================

pub trait SlotCallback<T, R>: Send + Sync + 'static {}
