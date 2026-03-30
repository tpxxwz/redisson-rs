use anyhow::{Result, anyhow};
use fred::prelude::Value;
use fred::types::FromValue;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

// ============================================================
// BatchHandle<T> — 对应 Java RFuture<T>（batch 模式下的延迟结果句柄）
// ============================================================

/// 同步入队后立即返回的结果句柄，对应 Java RFuture<T>。
/// 实现 Future<Output = Result<T>>，.await 时挂起等待 execute() 通过 oneshot 回填结果。
pub struct BatchHandle<T> {
    pub(crate) rx: oneshot::Receiver<Result<Value>>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: FromValue + Send> BatchHandle<T> {
    pub(crate) fn new(rx: oneshot::Receiver<Result<Value>>) -> Self {
        Self {
            rx,
            _marker: PhantomData,
        }
    }
}

impl<T: FromValue + Send> Future for BatchHandle<T> {
    type Output = Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.rx).poll(cx) {
            Poll::Ready(Ok(Ok(value))) => Poll::Ready(T::from_value(value).map_err(Into::into)),
            Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
            Poll::Ready(Err(_)) => {
                Poll::Ready(Err(anyhow!("batch command dropped before execute()")))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
