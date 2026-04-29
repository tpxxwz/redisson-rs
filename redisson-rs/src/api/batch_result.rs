// 对应 Java org.redisson.api.BatchResult

/// 对应 Java org.redisson.api.BatchResult<?>。
/// `execute_async()` 的返回值，包含所有命令的有序响应列表和同步从节点数。
#[derive(Debug)]
pub struct BatchResult {
    /// 按命令添加顺序排列的响应列表。
    /// skipResult 时为空列表。
    pub responses: Vec<fred::types::Value>,
    /// WAIT/WAITAOF 返回的已同步从节点数量。
    /// 无 WAIT 命令时为 0。
    pub synced_slaves: i64,
}

impl BatchResult {
    pub fn new(responses: Vec<fred::types::Value>, synced_slaves: i64) -> Self {
        Self { responses, synced_slaves }
    }
}
