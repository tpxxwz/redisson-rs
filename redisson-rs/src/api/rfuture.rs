use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

// ============================================================
// RFuture<T> — 对应 Java org.redisson.api.RFuture<T>（接口）
// ============================================================

pub type RFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>;
