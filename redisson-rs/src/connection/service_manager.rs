use crate::config::command_mapper::{self, CommandMapper};
use crate::config::equal_jitter_delay::{DelayStrategy, EqualJitterDelay};
use crate::config::name_mapper::NameMapper;
use crate::config::nat_mapper::{self, NatMapper};
use crate::config::RedissonConfig;
use crate::config::ServerMode;
use crate::connection::adder_entry::AdderEntry;
use crate::connection::connection_events_hub::ConnectionEventsHub;
use crate::connection::idle_connection_watcher::IdleConnectionWatcher;
use crate::connection::node_source::NodeSource;
use crate::connection::response_entry::ResponseEntry;
use crate::elements_subscribe_service::ElementsSubscribeService;
use crate::liveobject::resolver::map_resolver::MapResolver;
use crate::queue_transfer_service::QueueTransferService;
use crate::redisson_client_side_caching::RedissonClientSideCaching;
use crate::renewal::renewal_scheduler_trait::RenewalScheduler;
use anyhow::{Result, anyhow};
use dashmap::DashMap;
use parking_lot::RwLock;
use rand::RngCore;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

// ============================================================
// ServiceManager — 对应 Java org.redisson.connection.ServiceManager
// ============================================================

#[derive(Clone, Debug, Default)]
pub struct Timeout;

#[derive(Clone, Debug, Default)]
pub struct EventLoopGroup;

#[derive(Clone, Debug, Default)]
pub struct AddressResolverGroup;

#[derive(Clone, Debug, Default)]
pub struct ExecutorService;

#[derive(Clone, Debug, Default)]
pub struct HashedWheelTimer;

#[derive(Clone, Debug, Default)]
pub struct DuplexChannel;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RedisURI {
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

impl RedisURI {
    pub fn new(scheme: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        Self {
            scheme: scheme.into(),
            host: host.into(),
            port,
        }
    }
}

pub struct ServiceManager {
    pub(crate) connection_events_hub: ConnectionEventsHub,
    pub(crate) id: String,
    pub(crate) group: EventLoopGroup,
    pub(crate) resolver_group: AddressResolverGroup,
    pub(crate) executor: ExecutorService,
    pub(crate) cfg: Arc<RedissonConfig>,
    pub(crate) connection_watcher: IdleConnectionWatcher,
    pub(crate) elements_subscribe_service: ElementsSubscribeService,
    pub(crate) nat_mapper: RwLock<Arc<dyn NatMapper>>,
    pub(crate) responses: DashMap<String, ResponseEntry>,
    pub(crate) queue_transfer_service: QueueTransferService,
    pub(crate) renewal_scheduler: OnceLock<Arc<dyn RenewalScheduler>>,
    pub(crate) name_mapper: Arc<dyn NameMapper>,
    pub(crate) command_mapper: Arc<dyn CommandMapper>,
    pub(crate) timer: HashedWheelTimer,
    pub(crate) adders_usage: DashMap<String, AdderEntry>,
    pub(crate) adders_counter: DashMap<String, u32>,
    pub(crate) map_resolver: MapResolver,
    pub(crate) cluster_detected: AtomicBool,
    pub(crate) last_cluster_nodes: RwLock<Option<String>>,
    pub(crate) subscribe_timeout_ms: u64,
    pub(crate) command_timeout_ms: u64,
    pub(crate) retry_attempts: u32,
    pub(crate) retry_delay: EqualJitterDelay,
}

impl ServiceManager {
    pub fn new(
        name_mapper: Arc<dyn NameMapper>,
        cfg: Arc<RedissonConfig>,
        subscribe_timeout_ms: u64,
        command_timeout_ms: u64,
        retry_attempts: u32,
        retry_delay: EqualJitterDelay,
    ) -> Self {
        Self {
            connection_events_hub: ConnectionEventsHub,
            id: uuid::Uuid::new_v4().to_string(),
            group: EventLoopGroup,
            resolver_group: AddressResolverGroup,
            executor: ExecutorService,
            cfg,
            connection_watcher: IdleConnectionWatcher,
            elements_subscribe_service: ElementsSubscribeService,
            nat_mapper: RwLock::new(nat_mapper::direct()),
            responses: DashMap::new(),
            queue_transfer_service: QueueTransferService,
            renewal_scheduler: OnceLock::new(),
            name_mapper,
            command_mapper: command_mapper::direct(),
            timer: HashedWheelTimer,
            adders_usage: DashMap::new(),
            adders_counter: DashMap::new(),
            map_resolver: MapResolver,
            cluster_detected: AtomicBool::new(false),
            last_cluster_nodes: RwLock::new(None),
            subscribe_timeout_ms,
            command_timeout_ms,
            retry_attempts,
            retry_delay,
        }
    }

    pub fn new_timeout(&self, _delay_ms: u64) -> Timeout {
        Timeout
    }

    pub fn is_shutting_down(&self) -> bool {
        false
    }

    pub fn is_shutting_down_error(&self, _err: &anyhow::Error) -> bool {
        false
    }

    pub fn is_shutdown(&self) -> bool {
        false
    }

    pub fn connection_events_hub(&self) -> ConnectionEventsHub {
        self.connection_events_hub.clone()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn group(&self) -> EventLoopGroup {
        self.group.clone()
    }

    pub async fn resolve_all(&self, uri: RedisURI) -> Result<Vec<RedisURI>> {
        Ok(vec![self
            .nat_mapper()
            .map(self.to_uri(&uri.scheme, &uri.host, uri.port))])
    }

    pub fn resolver_group(&self) -> AddressResolverGroup {
        self.resolver_group.clone()
    }

    pub fn executor(&self) -> ExecutorService {
        self.executor.clone()
    }

    pub fn cfg(&self) -> &RedissonConfig {
        &self.cfg
    }

    pub fn timer(&self) -> HashedWheelTimer {
        self.timer.clone()
    }

    pub fn connection_watcher(&self) -> IdleConnectionWatcher {
        self.connection_watcher.clone()
    }

    pub fn socket_channel_class(&self) -> DuplexChannel {
        DuplexChannel
    }

    pub fn add_future<T>(&self, _future: T) {}

    pub fn shutdown_futures(&self, _timeout_ms: u64) {}

    pub fn close(&self) {}

    pub fn set_last_cluster_nodes(&self, last_cluster_nodes: String) {
        *self.last_cluster_nodes.write() = Some(last_cluster_nodes);
    }

    pub async fn create_node_not_found_future<T>(
        &self,
        channel_name: &str,
        slot: u16,
    ) -> Result<T> {
        Err(anyhow!(
            "Node for name: {} slot: {} hasn't been discovered yet. Last cluster nodes topology: {}",
            channel_name,
            slot,
            self.last_cluster_nodes
                .read()
                .clone()
                .unwrap_or_else(|| "<unknown>".to_string())
        ))
    }

    pub fn create_node_not_found_exception(&self, source: &NodeSource) -> anyhow::Error {
        anyhow!(
            "Node source hasn't been discovered yet. slot={:?}, addr={:?}, last_cluster_nodes={}",
            source.slot,
            source.addr,
            self.last_cluster_nodes
                .read()
                .clone()
                .unwrap_or_else(|| "<unknown>".to_string())
        )
    }

    pub fn config(&self) -> &RedissonConfig {
        self.cfg()
    }

    pub fn name_mapper(&self) -> &Arc<dyn NameMapper> {
        &self.name_mapper
    }

    pub fn command_mapper(&self) -> &Arc<dyn CommandMapper> {
        &self.command_mapper
    }

    pub fn elements_subscribe_service(&self) -> ElementsSubscribeService {
        self.elements_subscribe_service.clone()
    }

    pub async fn resolve_ip(&self, address: RedisURI) -> Result<RedisURI> {
        Ok(self.nat_mapper().map(address))
    }

    pub async fn resolve_ip_with_scheme(
        &self,
        scheme: &str,
        address: RedisURI,
    ) -> Result<RedisURI> {
        Ok(self.to_uri(scheme, &address.host, address.port))
    }

    pub async fn resolve(&self, address: RedisURI) -> Result<String> {
        Ok(format!("{}:{}", address.host, address.port))
    }

    pub fn to_uri(&self, scheme: &str, host: &str, port: u16) -> RedisURI {
        self.nat_mapper()
            .map(RedisURI::new(scheme.to_string(), host.to_string(), port))
    }

    pub fn set_nat_mapper(&self, nat_mapper: Arc<dyn NatMapper>) {
        *self.nat_mapper.write() = nat_mapper;
    }

    pub fn nat_mapper(&self) -> Arc<dyn NatMapper> {
        self.nat_mapper.read().clone()
    }

    pub fn is_cached(&self, _addr: &str, _script: &str) -> bool {
        false
    }

    pub fn cache_scripts(&self, _addr: &str, _scripts: &[String]) {}

    pub fn calc_sha(&self, script: &str) -> String {
        fred::util::sha1_hash(script)
    }

    pub async fn execute<T, F, Fut>(&self, supplier: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        supplier().await
    }

    pub async fn transfer<T, F>(&self, source: F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
    {
        source.await
    }

    pub async fn transfer_exception<T, F>(&self, source: F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
    {
        source.await
    }

    pub fn random(&self) -> rand::rngs::ThreadRng {
        rand::thread_rng()
    }

    pub fn generate_value(&self) -> u64 {
        rand::random()
    }

    pub fn generate_id(&self) -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    pub fn generate_id_array(&self) -> Vec<u8> {
        self.generate_id_array_sized(16)
    }

    pub fn generate_id_array_sized(&self, size: usize) -> Vec<u8> {
        let mut id = vec![0_u8; size];
        let mut rng = self.random();
        rng.fill_bytes(&mut id);
        id
    }

    pub fn live_object_latch(&self) -> bool {
        false
    }

    pub fn is_resp3(&self) -> bool {
        false
    }

    pub fn resp3<T>(&self, command: T) -> T {
        command
    }

    pub fn responses(&self) -> DashMap<String, ResponseEntry> {
        self.responses.clone()
    }

    pub fn queue_transfer_service(&self) -> QueueTransferService {
        self.queue_transfer_service.clone()
    }

    pub fn codec<T>(&self, codec: Option<T>) -> Option<T> {
        codec
    }

    pub fn adders_usage(&self) -> DashMap<String, AdderEntry> {
        self.adders_usage.clone()
    }

    pub fn adders_counter(&self) -> DashMap<String, u32> {
        self.adders_counter.clone()
    }

    pub fn live_object_map_resolver(&self) -> MapResolver {
        self.map_resolver.clone()
    }

    pub fn register(&self, renewal_scheduler: Arc<dyn RenewalScheduler>) {
        let _ = self.renewal_scheduler.set(renewal_scheduler);
    }

    pub fn renewal_scheduler(&self) -> &Arc<dyn RenewalScheduler> {
        self.renewal_scheduler
            .get()
            .expect("renewal scheduler not registered")
    }

    pub fn add_client_side_caching(
        &self,
        _caching_instance: Arc<dyn RedissonClientSideCaching>,
    ) {
    }

    pub fn remove_client_side_caching(
        &self,
        _caching_instance: Arc<dyn RedissonClientSideCaching>,
    ) {
    }

    pub fn has_caching_instances(&self) -> bool {
        false
    }

    pub fn evict_client_side_caching(&self, _name: &str) {}

    pub fn set_cluster_detected(&self, cluster_detected: bool) {
        self.cluster_detected.store(cluster_detected, Ordering::Release);
    }

    pub fn is_cluster_setup(&self) -> bool {
        self.is_cluster_config() || self.cluster_detected.load(Ordering::Acquire)
    }

    // ============================================================
    // Rust 当前实现额外使用的 helper
    // ============================================================

    pub fn subscribe_timeout_ms(&self) -> u64 {
        self.subscribe_timeout_ms
    }

    pub fn calc_unlock_latch_timeout_ms(&self) -> u64 {
        let delay = self.retry_delay.calc_delay(self.retry_attempts);
        let timeout = (self.command_timeout_ms + delay) * self.retry_attempts as u64;
        timeout.max(1)
    }

    pub fn is_cluster_config(&self) -> bool {
        matches!(&self.cfg.mode, ServerMode::Cluster { .. })
    }
}
