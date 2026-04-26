use crate::command::command_async_service::CommandAsyncServiceLike;
use crate::connection::fred_connection_manager::FredConnectionManager;
use crate::connection::service_manager::ServiceManager;
use std::sync::Arc;

pub trait CommandAsyncExecutor: Send + Sync {
    fn connection_manager(&self) -> Arc<FredConnectionManager>;
    fn service_manager(&self) -> Arc<ServiceManager>;
    fn is_track_changes(&self) -> bool;
}

impl<T: CommandAsyncServiceLike> CommandAsyncExecutor for T {
    fn connection_manager(&self) -> Arc<FredConnectionManager> {
        self.inner().connection_manager.clone()
    }

    fn service_manager(&self) -> Arc<ServiceManager> {
        self.inner().connection_manager.service_manager().clone()
    }

    fn is_track_changes(&self) -> bool {
        self.inner().track_changes
    }
}
