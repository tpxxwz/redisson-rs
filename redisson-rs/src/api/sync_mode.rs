// 对应 Java org.redisson.api.SyncMode

/// 对应 Java SyncMode，控制 synced_eval 使用哪种从库同步机制。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// 自动选择：AOF 可用时走 WAITAOF，否则走 WAIT，均不支持时无同步。
    Auto,
    /// 只用 WAIT，等待从库内存确认。
    Wait,
    /// 只用 WAITAOF，等待 AOF 持久化确认（Redis 7.2+）。
    WaitAof,
}
