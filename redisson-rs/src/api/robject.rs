use crate::command::command_async_executor::CommandAsyncExecutor;
use crate::command::command_async_service::CommandAsyncService;
use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;

pub trait RObject: Send + Sync {
    fn object_inner(&self) -> &RedissonObjectInner;

    async fn get_name(&self) -> String;

    async fn delete(&self) -> bool;
}

struct RedissonObjectInner {
    pub(crate) command_executor: Arc<dyn CommandAsyncExecutor>,
    pub(crate) name: RwLock<String>,
}
