use crate::redisson::Redisson;
use anyhow::Result;
use fred::prelude::Expiration;
use std::sync::Arc;

// ============================================================
// RedisKey trait
// ============================================================

pub trait RedisKey {
    fn key(&self) -> String;
}

impl RedisKey for &str {
    fn key(&self) -> String {
        self.to_string()
    }
}

impl RedisKey for String {
    fn key(&self) -> String {
        self.clone()
    }
}

// ============================================================
// RedisExt — 实现在 Arc<Redisson> 上
// ============================================================

pub trait RedisExt {
    fn get_json<T, K>(&self, key: K) -> impl std::future::Future<Output = Result<Option<T>>> + Send
    where
        T: serde::de::DeserializeOwned + Send,
        K: RedisKey + Send;

    fn set_json<T, K>(
        &self,
        key: K,
        value: &T,
        expire_secs: Option<i64>,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        T: serde::Serialize + Send + Sync,
        K: RedisKey + Send;
}

impl RedisExt for Arc<Redisson> {
    async fn get_json<T, K>(&self, key: K) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned + Send,
        K: RedisKey + Send,
    {
        let raw = self.command_executor().get_str(&key.key()).await?;
        match raw {
            Some(json) if !json.is_empty() => Ok(Some(serde_json::from_str(&json)?)),
            _ => Ok(None),
        }
    }

    async fn set_json<T, K>(&self, key: K, value: &T, expire_secs: Option<i64>) -> Result<()>
    where
        T: serde::Serialize + Send + Sync,
        K: RedisKey + Send,
    {
        let json = serde_json::to_string(value)?;
        let expire = expire_secs.filter(|&s| s > 0).map(Expiration::EX);
        self.command_executor()
            .set_value(&key.key(), json, expire)
            .await
    }
}
