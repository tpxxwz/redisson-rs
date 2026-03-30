// ============================================================
// RedissonObjectBuilder — 对应 Java org.redisson.liveobject.core.RedissonObjectBuilder（类）
// ============================================================

/// 对应 Java org.redisson.liveobject.core.RedissonObjectBuilder。
/// 负责将 Live Object 字段与 Redis 数据结构建立映射关系。
pub struct RedissonObjectBuilder;

/// 对应 Java org.redisson.liveobject.core.RedissonObjectBuilder.ReferenceType（内部枚举）。
pub enum ReferenceType {
    RxJava,
    Reactive,
    Default,
}
