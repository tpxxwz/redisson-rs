mod test_support;

use fred::interfaces::ClientLike;
use redisson_rs::config::build_fred_config;
use redisson_rs::{RPatternTopic, Redisson};
use redisson_rs::PatternMessageListener;
use redisson_rs::command::ListenerMessage;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use test_support::{local_cluster_config, local_standalone_config};
use crate::test_support::local_sentinel_config;
// ─────────────────────────────────────────────────────────────

struct RecordingListener {
    received: Arc<Mutex<Vec<String>>>,
}

impl RecordingListener {
    fn new() -> (Arc<Self>, Arc<Mutex<Vec<String>>>) {
        let received = Arc::new(Mutex::new(Vec::new()));
        (Arc::new(Self { received: received.clone() }), received)
    }
}

impl PatternMessageListener for RecordingListener {
    fn on_message(&self, pattern: &str, channel: &str, msg: ListenerMessage) {
        println!("[pubsub] pattern={} channel={} msg={:?}", pattern, channel, msg);
        self.received.lock().unwrap().push(channel.to_string());
    }
}

/// add_listener 后 remove_listener 应正常完成，不 panic。
#[tokio::test]
async fn test_add_and_remove_listener() {
    let (_env, config) = local_standalone_config(false).await;
    let redisson = Redisson::create(config).await.expect("create");
    let topic = redisson.get_pattern_topic("test.cleanup.*");
    let (listener, _) = RecordingListener::new();

    let id = topic.add_listener(listener).await.expect("add_listener");
    topic.remove_listener(id).await.expect("remove_listener");
}

/// publish 后，匹配 pattern 的 listener 应收到消息。
#[tokio::test]
async fn test_receives_published_message() {
    use fred::prelude::*;
    use fred::interfaces::PubsubInterface;

    let (_env, config) = local_standalone_config(false).await;
    let redisson = Redisson::create(config.clone()).await.expect("create");
    let topic = redisson.get_pattern_topic("msg.*");
    let (listener, received) = RecordingListener::new();

    topic.add_listener(listener).await.expect("add_listener");

    let fred_config = build_fred_config(&config).expect("fred config");
    let client = Client::new(fred_config, None, None, None);
    client.init().await.expect("client init");
    let _: i64 = client.publish("msg.hello", "world").await.expect("publish");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let msgs = received.lock().unwrap();
    assert!(msgs.iter().any(|ch| ch == "msg.hello"), "got: {:?}", *msgs);
}

/// 验证 local_cluster_config 能正常启动并用 fred cluster client 连接成功。
#[tokio::test]
async fn test_cluster_startup() {
    use fred::interfaces::KeysInterface;

    let (_env, config) = local_cluster_config(false).await;
    println!("[startup] cluster config built, connecting...");

    let fred_config = build_fred_config(&config).expect("fred config");
    let client = fred::clients::Client::new(fred_config, None, None, None);
    client.init().await.expect("cluster client init");

    // 简单 set/get 验证连通性
    let _: () = client.set("__startup_probe__", "ok", None, None, false).await.expect("set");
    let val: Option<String> = client.get("__startup_probe__").await.expect("get");
    assert_eq!(val.as_deref(), Some("ok"));
    assert_eq!(val.as_deref(), Some("ok"));
    println!("[startup] cluster is up, set/get succeeded");
    client.quit().await.ok();
}

/// cluster 模式下监听 keyspace 事件：
/// 对分布在不同 master 节点的多个 key 执行 set / del / expire，
/// 验证所有 key 的 keyevent 通知都能收到。
#[tokio::test]
async fn test_cluster_keyspace_listener() {
    use fred::interfaces::KeysInterface;
    use redisson_rs::command::ListenerMessage;

    let (_env, config) = local_cluster_config(true).await;
    // let (_env, config) = local_standalone_config(true).await;
    // let (_env, config) = local_sentinel_config(true).await;

    let fred_config = build_fred_config(&config).expect("fred config");
    let client = fred::clients::Client::new(fred_config, None, None, None);
    client.init().await.expect("client init");
    let db = match &config.mode {
        redisson_rs::config::server_mode::ServerMode::Standalone { db, .. } => *db,
        redisson_rs::config::server_mode::ServerMode::Sentinel { db, .. } => *db,
        redisson_rs::config::server_mode::ServerMode::Cluster { .. } => 0,
    };

    // 用 Arc<Mutex> 收集收到的 key 名（keyevent 里 msg = key 名）
    let received_keys: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let rx = received_keys.clone();
    let redisson = Redisson::create(config).await.expect("create");
    // keyevent pattern 必须和实际 DB 对齐，standalone/sentinel 切库后不能写死 @0。
    let topic = redisson.get_pattern_topic(&format!("__keyevent@{}__:*", db));
    topic.add_listener(Arc::new(move |_pattern: &str, channel: &str, msg: ListenerMessage| {
        if let ListenerMessage::String(key) = msg {
            println!("[keyevent] event={} key={}", channel, key);
            rx.lock().unwrap().push(key.to_string());
        }
    })).await.expect("add_listener");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // 这几个 key 的 CRC16 hash 落在不同 slot，会路由到不同 master 节点
    // alpha→7794(master1)  beta→14539(master2)  gamma→1794(master0)
    // delta→11298(master2) foo→12356(master2)   bar→5061(master0)
    let set_keys = ["alpha", "beta", "gamma", "delta", "foo", "bar"];
    for key in &set_keys {
        let _: () = client.set(*key, "v", None, None, false).await.expect("set");
    }
    // 额外触发 del 和 expire，产生更多事件类型
    let _: i64 = client.del("alpha").await.expect("del");
    let _: bool = client.expire("beta", 1, None).await.expect("expire");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let keys = received_keys.lock().unwrap();
    for key in &set_keys {
        assert!(
            keys.contains(&key.to_string()),
            "missing keyevent for key={key}, received={keys:?}",
        );
    }
}
