use std::collections::{HashMap, VecDeque};

// ============================================================
// LockEntry — 对应 Java org.redisson.renewal.LockEntry
// ============================================================

/// 单把锁的续约信息，对应 Java LockEntry。
/// 维护 per-thread 计数和有序队列，支持可重入场景下多个 task 同时持有同一把锁。
pub struct LockEntry {
    /// 对应 Java threadId2lockName：thread_id → lock_name
    thread2lock_name: HashMap<String, String>,
    /// 对应 Java threadId2counter：thread_id → 重入计数
    thread_counter: HashMap<String, usize>,
    /// 对应 Java threadsQueue：有序队列，重入时同一 thread_id 可多次入队
    threads_queue: VecDeque<String>,
}

impl LockEntry {
    pub fn new() -> Self {
        Self {
            thread2lock_name: HashMap::new(),
            thread_counter: HashMap::new(),
            threads_queue: VecDeque::new(),
        }
    }

    /// 对应 Java LockEntry.addThreadId(threadId, lockName)
    pub fn add_thread_id(&mut self, thread_id: String, lock_name: String) {
        let counter = self.thread_counter.entry(thread_id.clone()).or_insert(0);
        *counter += 1;
        self.threads_queue.push_back(thread_id.clone());
        self.thread2lock_name.entry(thread_id).or_insert(lock_name);
    }

    /// 对应 Java LockEntry.removeThreadId(threadId)
    pub fn remove_thread_id(&mut self, thread_id: &str) {
        let is_zero = if let Some(counter) = self.thread_counter.get_mut(thread_id) {
            *counter -= 1;
            *counter == 0
        } else {
            return;
        };
        // counter > 0 时不清理队列（对应 Java 行为：queue 仅在 counter==0 时 removeIf）
        if is_zero {
            self.thread_counter.remove(thread_id);
            self.thread2lock_name.remove(thread_id);
            self.threads_queue.retain(|t| t != thread_id);
        }
    }

    /// 对应 Java LockEntry.hasNoThreads()
    pub fn has_no_threads(&self) -> bool {
        self.threads_queue.is_empty()
    }

    /// 对应 Java LockEntry.getFirstThreadId() + getLockName(threadId)：
    /// 返回队列第一个 thread 的 lock_name，用于续约脚本的 hexists 检查
    pub fn get_first_lock_name(&self) -> Option<&str> {
        self.threads_queue
            .front()
            .and_then(|tid| self.thread2lock_name.get(tid))
            .map(|s| s.as_str())
    }
}
