use std::collections::HashSet;

/// 对应 Java 中 addListener 返回的 int listener id
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId {
    pub(crate) id: usize,
}

impl From<usize> for ListenerId {
    fn from(id: usize) -> Self {
        Self { id }
    }
}

impl From<ListenerId> for usize {
    fn from(lid: ListenerId) -> Self {
        lid.id
    }
}

/// 支持单个或多个 ListenerId，对应 Java removeListenerAsync(int... listenerIds)
pub struct MultipleListenerIds {
    ids: HashSet<ListenerId>,
}

impl MultipleListenerIds {
    pub fn contains(&self, id: &ListenerId) -> bool {
        self.ids.contains(id)
    }
}

impl From<ListenerId> for MultipleListenerIds {
    fn from(id: ListenerId) -> Self {
        Self { ids: std::iter::once(id).collect() }
    }
}

impl From<Vec<ListenerId>> for MultipleListenerIds {
    fn from(ids: Vec<ListenerId>) -> Self {
        Self { ids: ids.into_iter().collect() }
    }
}

impl From<HashSet<ListenerId>> for MultipleListenerIds {
    fn from(ids: HashSet<ListenerId>) -> Self {
        Self { ids }
    }
}
