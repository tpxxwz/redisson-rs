mod test_support;

use fred::clients::{Client, SubscriberClient};
use fred::interfaces::{ClientLike, EventInterface, KeysInterface, PubsubInterface};
use fred::prelude::ReconnectPolicy;
use redisson_rs::config::build_fred_config;
use std::time::Duration;
use crate::test_support::local_cluster_config;

#[tokio::test]
async fn test_fred_cluster_node_psubscribe_message_rx() {
    // let (_env, config) = local_standalone_config().await;
    let (_env, config) = local_cluster_config(true).await;
    let fred_config = build_fred_config(&config).expect("fred config");

    let client = Client::new(
        fred_config.clone(),
        None,
        None,
        Some(ReconnectPolicy::new_constant(10, 500)),
    );
    client.init().await.expect("client init");

    let subscriber = SubscriberClient::new(
        fred_config,
        None,
        None,
        Some(ReconnectPolicy::new_exponential(0, 1, 100, 2)),
    );
    subscriber.init().await.expect("subscriber init");

    let mut rx = subscriber.keyspace_event_rx();
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
            Ok(Ok(event)) => {
                println!(
                    "[fred-direct] db={} operation={} key={:?}",
                    event.db, event.operation, event.key
                );
                received.push(String::from_utf8_lossy(event.key.as_bytes()).to_string());
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    println!("[fred-direct] received keys: {:?}", received);
}
