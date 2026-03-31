// ============================================================
// BatchResult<E> — 对应 Java org.redisson.api.BatchResult<E>（final 类）
// ============================================================

/// 对应 Java org.redisson.api.BatchResult<E>。
/// 持有 RBatch.execute() 的最终结果：各命令响应列表 + 已同步从节点数。
#[derive(Debug)]
pub struct BatchResult<E> {
    /// 对应 Java BatchResult.responses（各命令按加入顺序排列的响应）
    pub responses: Vec<E>,
    /// 对应 Java BatchResult.syncedSlaves（实际同步的从节点数，WAIT 命令返回值）
    pub synced_slaves: i32,
}

impl<E> BatchResult<E> {
    /// 对应 Java new BatchResult(List<E> responses, int syncedSlaves)
    pub fn new(responses: Vec<E>, synced_slaves: i32) -> Self {
        Self { responses, synced_slaves }
    }

    /// 对应 Java BatchResult.getResponses()
    pub fn get_responses(&self) -> &[E] {
        &self.responses
    }

    /// 对应 Java BatchResult.getSyncedSlaves()
    pub fn get_synced_slaves(&self) -> i32 {
        self.synced_slaves
    }
}
