#![allow(dead_code)]

use fred::clients::Client;
use fred::interfaces::{ClientLike, ConfigInterface};
use redisson_rs::config::{RedisConfig, RedisNode, RedissonConfig, build_fred_config};
use redisson_rs::config::server_mode::ServerMode;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};
use testcontainers_modules::redis::Redis;

/// 类型擦除的容器/compose 持有者，drop 时自动清理。
pub struct TestEnv(pub Box<dyn std::any::Any + Send + Sync>);

async fn finalize_test_config(
    env: TestEnv,
    redisson_config: RedissonConfig,
    enable_keyspace_events: bool,
) -> (TestEnv, RedissonConfig) {
    if enable_keyspace_events {
        configure_keyspace_events(&redisson_config).await;
    }

    (env, redisson_config)
}

pub async fn local_standalone_config(enable_keyspace_events: bool) -> (TestEnv, RedissonConfig) {
    let container = Redis::default()
        .start()
        .await
        .expect("standalone redis start");

    let port = container.get_host_port_ipv4(6379).await.expect("get port");

    let mut config = RedisConfig::default();
    config.port = port;
    config.db = 15;
    let redisson_config = RedissonConfig::try_from(config).expect("build config");

    finalize_test_config(
        TestEnv(Box::new(container)),
        redisson_config,
        enable_keyspace_events,
    )
    .await
}

/// 在宿主机上找 `count` 个连续可用端口。
/// 用同号映射（宿主机端口 = 容器端口），Redis 节点 announce 的地址内外一致，
/// 无需动 cluster-announce-port，slave 复制也不会出现 Connection refused。
fn find_consecutive_free_ports(count: usize) -> u16 {
    'outer: for base in 10000u16..=(49000 - count as u16) {
        for offset in 0..count {
            if std::net::TcpListener::bind(("127.0.0.1", base + offset as u16)).is_err() {
                continue 'outer;
            }
        }
        return base;
    }
    panic!("cannot find {} consecutive free ports", count)
}

fn local_ipv4() -> std::net::Ipv4Addr {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).expect("udp bind");
    socket.connect(("8.8.8.8", 80)).expect("udp connect");
    match socket.local_addr().expect("local addr").ip() {
        std::net::IpAddr::V4(ip) => ip,
        std::net::IpAddr::V6(_) => panic!("expected IPv4 address"),
    }
}

pub async fn local_cluster_config(enable_keyspace_events: bool) -> (TestEnv, RedissonConfig) {
    // 预找 6 个连续空闲端口，通过 INITIAL_PORT 让 grokzen 用这些端口启动，
    // 同时用 with_mapped_port 做同号映射（host:N → container:N）。
    // 这样容器内 slave→master 复制走的是 127.0.0.1:N（容器内可达），
    // 宿主机客户端收到的 MOVED 重定向也是 127.0.0.1:N（宿主机可达），两边一致，无需 announce 修改。
    let base = find_consecutive_free_ports(6);

    let container = GenericImage::new("grokzen/redis-cluster", "7.0.0")
        .with_wait_for(WaitFor::message_on_stdout("Cluster state changed: ok"))
        .with_exposed_port(base.tcp())
        .with_exposed_port((base + 1).tcp())
        .with_exposed_port((base + 2).tcp())
        .with_exposed_port((base + 3).tcp())
        .with_exposed_port((base + 4).tcp())
        .with_exposed_port((base + 5).tcp())
        .with_mapped_port(base, base.tcp())
        .with_mapped_port(base + 1, (base + 1).tcp())
        .with_mapped_port(base + 2, (base + 2).tcp())
        .with_mapped_port(base + 3, (base + 3).tcp())
        .with_mapped_port(base + 4, (base + 4).tcp())
        .with_mapped_port(base + 5, (base + 5).tcp())
        .with_env_var("IP", "127.0.0.1")
        .with_env_var("MASTERS", "3")
        .with_env_var("SLAVES_PER_MASTER", "1")
        .with_env_var("INITIAL_PORT", base.to_string())
        .start()
        .await
        .expect("cluster redis start");

    let nodes: Vec<RedisNode> = (0..3u16)
        .map(|i| RedisNode {
            host: "127.0.0.1".to_string(),
            port: base + i,
        })
        .collect();

    let mut config = RedisConfig::default();
    config.mode = "cluster".to_string();
    config.nodes = nodes;
    let redisson_config = RedissonConfig::try_from(config).expect("build config");

    finalize_test_config(
        TestEnv(Box::new(container)),
        redisson_config,
        enable_keyspace_events,
    )
    .await
}

pub async fn local_sentinel_config(enable_keyspace_events: bool) -> (TestEnv, RedissonConfig) {
    let master = GenericImage::new("redis", "7.0")
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .with_exposed_port(6379.tcp())
        .start()
        .await
        .expect("redis master start");

    let master_port = master.get_host_port_ipv4(6379).await.expect("master port");
    let host_ip = local_ipv4();
    let sentinel_cmd = format!(
        "cat >/tmp/sentinel.conf <<'EOF'\n\
         port 26379\n\
         bind 0.0.0.0\n\
         sentinel monitor mymaster {host_ip} {master_port} 1\n\
         sentinel down-after-milliseconds mymaster 5000\n\
         sentinel failover-timeout mymaster 10000\n\
         sentinel parallel-syncs mymaster 1\n\
         EOF\n\
         exec redis-server /tmp/sentinel.conf --sentinel"
    );

    let sentinel = GenericImage::new("redis", "7.0")
        .with_wait_for(WaitFor::message_on_stdout("monitor master mymaster"))
        .with_exposed_port(26379.tcp())
        .with_entrypoint("sh")
        .with_cmd(["-c", &sentinel_cmd])
        .start()
        .await
        .expect("redis sentinel start");

    let sentinel_port = sentinel
        .get_host_port_ipv4(26379)
        .await
        .expect("sentinel port");

    let mut config = RedisConfig::default();
    config.mode = "sentinel".to_string();
    config.sentinel_service_name = "mymaster".to_string();
    config.nodes = vec![RedisNode {
        host: "127.0.0.1".to_string(),
        port: sentinel_port,
    }];
    config.db = 15;
    let redisson_config = RedissonConfig::try_from(config).expect("build config");

    finalize_test_config(
        TestEnv(Box::new((master, sentinel))),
        redisson_config,
        enable_keyspace_events,
    )
    .await
}

pub async fn configure_keyspace_events(config: &RedissonConfig) {
    let nodes: Vec<RedisNode> = match &config.mode {
        ServerMode::Standalone { server, .. } => vec![server.clone()],
        ServerMode::Cluster { nodes } => nodes.clone(),
        ServerMode::Sentinel { .. } => {
            let fred_config = build_fred_config(config).expect("fred config");
            let client = Client::new(fred_config, None, None, None);
            client.init().await.expect("client init");
            client
                .config_set("notify-keyspace-events", "KEA")
                .await
                .expect("config set");
            client.quit().await.ok();
            return;
        }
    };

    for node in nodes {
        let mut single = RedisConfig::default();
        single.host = node.host;
        single.port = node.port;
        single.username = config.username.clone();
        single.password = config.password.clone();

        let single_config = RedissonConfig::try_from(single).expect("build single-node config");
        let fred_config = build_fred_config(&single_config).expect("fred config");
        let client = Client::new(fred_config, None, None, None);
        client.init().await.expect("client init");
        client
            .config_set("notify-keyspace-events", "KEA")
            .await
            .expect("config set");
        client.quit().await.ok();
    }
}
