mod test_support;

use fred::clients::{Client, SubscriberClient};
use fred::interfaces::{ClientLike, ConfigInterface, EventInterface, KeysInterface, PubsubInterface};
use fred::prelude::ReconnectPolicy;
use fred::types::MessageKind;
use redisson_rs::config::build_fred_config;
use std::time::Duration;
use test_support::local_standalone_config;

#[tokio::test]
async fn test_fred_cluster_node_psubscribe_message_rx() {
    let (_env, config) = local_standalone_config().await;
    let fred_config = build_fred_config(&config).expect("fred config");

    let client = Client::new(fred_config.clone(), None, None, None);
    client.init().await.expect("client init");
    client
        .config_set("notify-keyspace-events", "KEA")
        .await
        .expect("config set");

    let subscriber = SubscriberClient::new(
        fred_config,
        None,
        None,
        Some(ReconnectPolicy::new_exponential(0, 1, 100, 2)),
    );
    subscriber.init().await.expect("subscriber init");

    let mut rx = subscriber.message_rx();
    subscriber
        .to_client()
        .psubscribe("__keyevent@0__:*")
        .await
        .expect("psubscribe");

    tokio::time::sleep(Duration::from_millis(200)).await;

    for key in ["alpha", "beta", "gamma"] {
        let _: () = client.set(key, "v", None, None, false).await.expect("set");
        let value: Option<String> = client.get(key).await.expect("get");
        println!("[fred-direct] set/get key={} value={:?}", key, value);
        assert_eq!(value.as_deref(), Some("v"));
    }

    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remain, rx.recv()).await {
            Ok(Ok(msg)) => {
                println!(
                    "[fred-direct] kind={:?} channel={} value={:?}",
                    msg.kind, msg.channel, msg.value
                );
                if msg.kind == MessageKind::PMessage {
                    received.push(msg.channel.to_string());
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    println!("[fred-direct] received channels: {:?}", received);
}
