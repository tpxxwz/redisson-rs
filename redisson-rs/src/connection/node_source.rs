// ============================================================
// NodeSource — 对应 Java org.redisson.connection.NodeSource
// ============================================================

#[derive(Clone, Debug, Default)]
pub struct NodeSource {
    pub slot: Option<u16>,
    pub addr: Option<String>,
}
