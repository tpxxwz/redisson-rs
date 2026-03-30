// ============================================================
// DelayStrategy — 对应 Java org.redisson.config.DelayStrategy
// ============================================================

/// 重试延迟策略接口，对应 Java DelayStrategy。
pub trait DelayStrategy {
    /// 计算第 attempt 次重试前的等待时长（ms），对应 Java DelayStrategy.calcDelay(int attempt)
    fn calc_delay(&self, attempt: u32) -> u64;
}

// ============================================================
// EqualJitterDelay — 对应 Java org.redisson.config.EqualJitterDelay
// ============================================================

/// Equal Jitter 指数退避策略，对应 Java EqualJitterDelay。
/// 保留指数退避延迟的一半作为固定部分，另一半引入随机抖动。
/// 结果在 [exponential/2, exponential) 之间，兼顾稳定性与随机性。
#[derive(Clone)]
pub struct EqualJitterDelay {
    /// 对应 Java EqualJitterDelay.baseDelay（ms）
    base_delay_ms: u64,
    /// 对应 Java EqualJitterDelay.maxDelay（ms）
    max_delay_ms: u64,
}

impl EqualJitterDelay {
    /// 对应 Java new EqualJitterDelay(baseDelay, maxDelay)
    pub fn new(base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            base_delay_ms,
            max_delay_ms,
        }
    }

    pub fn base_delay_ms(&self) -> u64 {
        self.base_delay_ms
    }

    pub fn max_delay_ms(&self) -> u64 {
        self.max_delay_ms
    }
}

impl DelayStrategy for EqualJitterDelay {
    /// 对应 Java EqualJitterDelay.calcDelay(int attempt)
    fn calc_delay(&self, attempt: u32) -> u64 {
        let base_ms = self.base_delay_ms;
        let max_ms = self.max_delay_ms;

        let exponential_ms = if attempt >= 63 || base_ms >= max_ms {
            max_ms
        } else {
            let shifted = 1u64 << attempt;
            if shifted > max_ms / base_ms {
                max_ms
            } else {
                (base_ms * shifted).min(max_ms)
            }
        };

        let half_delay = exponential_ms / 2;
        let random_component = if half_delay == 0 {
            0
        } else {
            rand::random::<u64>() % (half_delay + 1)
        };
        half_delay + random_component
    }
}
