// 对应 Java org.redisson.client.protocol.RedisCommands
//
// Java 的 RedisCommands 是一个接口，所有命令定义为静态字段，每个字段携带命令名和响应解码器。
// Rust 这边用枚举代替，每个变体携带类型化参数；解码由 fred 的 FromValue 泛型负责，不需要单独的解码器字段。
//
// execute() 方法封装了 fred 各 interface trait 的调用，以及需要降级探测的特殊逻辑
//（SORT_RO、EVALSHA_RO）。

use crate::command::command_async_service::{EVAL_SHA_RO_SUPPORTED, SORT_RO_SUPPORTED};
use fred::interfaces::{
    ClientLike, HashesInterface, KeysInterface, LuaInterface, ServerInterface, SetsInterface,
    SortedSetsInterface,
};
use fred::prelude::Pool;
use fred::types::config::Options;
use fred::types::{ClusterHash, CustomCommand, Expiration, Key, MultipleKeys, MultipleValues, SetOptions, Value};
use std::sync::atomic::Ordering;

// ============================================================
// RedisCommand — 对应 Java RedisCommands 接口中的各静态字段
// ============================================================
pub enum RedisCommand {

    // --------------------------------------------------------
    // String / generic key 命令
    // 对应 Java: GET, SET, DEL, EXISTS
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.GET
    Get {
        key: Key,
    },

    /// 对应 Java RedisCommands.SET / SET_VOID / SETNX / SET_BOOLEAN
    Set {
        key:     Key,
        value:   Value,
        expiry:  Option<Expiration>,
        options: Option<SetOptions>,
    },

    /// 对应 Java RedisCommands.DEL_VOID / DEL_OBJECTS
    Del {
        keys: MultipleKeys,
    },

    /// 对应 Java RedisCommands.EXISTS
    Exists {
        keys: MultipleKeys,
    },

    // --------------------------------------------------------
    // 过期相关
    // 对应 Java: EXPIRE, PEXPIRE, TTL, PTTL, PERSIST
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.EXPIRE
    Expire {
        key:     Key,
        seconds: i64,
    },

    /// 对应 Java RedisCommands.PEXPIRE
    Pexpire {
        key:          Key,
        milliseconds: i64,
    },

    /// 对应 Java RedisCommands.TTL
    Ttl {
        key: Key,
    },

    /// 对应 Java RedisCommands.PTTL
    Pttl {
        key: Key,
    },

    /// 对应 Java RedisCommands.PERSIST
    Persist {
        key: Key,
    },

    // --------------------------------------------------------
    // Hash 命令
    // 对应 Java: HGET, HSET, HGETALL, HDEL, HEXISTS, HLEN, HINCRBY
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.HGET
    HGet {
        key:   Key,
        field: Key,
    },

    /// 对应 Java RedisCommands.HSET / HSET_VOID
    HSet {
        key:   Key,
        field: Key,
        value: Value,
    },

    /// 对应 Java RedisCommands.HGETALL
    HGetAll {
        key: Key,
    },

    /// 对应 Java RedisCommands.HDEL
    HDel {
        key:    Key,
        fields: MultipleKeys,
    },

    /// 对应 Java RedisCommands.HEXISTS
    HExists {
        key:   Key,
        field: Key,
    },

    /// 对应 Java RedisCommands.HLEN
    HLen {
        key: Key,
    },

    /// 对应 Java RedisCommands.HINCRBY
    HIncrBy {
        key:   Key,
        field: Key,
        delta: i64,
    },

    // --------------------------------------------------------
    // Set 命令
    // 对应 Java: SADD, SREM, SMEMBERS, SISMEMBER, SCARD
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.SADD / SADD_BOOL
    SAdd {
        key:     Key,
        members: MultipleValues,
    },

    /// 对应 Java RedisCommands.SREM / SREM_SINGLE
    SRem {
        key:     Key,
        members: MultipleValues,
    },

    /// 对应 Java RedisCommands.SMEMBERS
    SMembers {
        key: Key,
    },

    /// 对应 Java RedisCommands.SISMEMBER
    SIsMember {
        key:    Key,
        member: Value,
    },

    /// 对应 Java RedisCommands.SCARD
    SCard {
        key: Key,
    },

    // --------------------------------------------------------
    // Sorted Set 命令
    // 对应 Java: ZADD, ZREM, ZSCORE, ZCARD
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.ZADD / ZADD_INT / ZADD_BOOL
    ZAdd {
        key:     Key,
        score:   f64,
        member:  Value,
        options: Option<SetOptions>,
    },

    /// 对应 Java RedisCommands.ZREM
    ZRem {
        key:     Key,
        members: MultipleValues,
    },

    /// 对应 Java RedisCommands.ZSCORE
    ZScore {
        key:    Key,
        member: Value,
    },

    /// 对应 Java RedisCommands.ZCARD
    ZCard {
        key: Key,
    },

    // --------------------------------------------------------
    // Lua 脚本命令
    // 对应 Java: EVAL_LONG / EVAL_BOOLEAN / EVAL_OBJECT 等（命令名均为 "EVAL" 或 "EVALSHA"）
    //
    // Java 里同一个 Redis 命令因返回类型不同定义了多个字段（EVAL_LONG、EVAL_BOOLEAN...），
    // Rust 这边返回类型由调用方的 FromValue 泛型处理，所以只需一个变体。
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.EVAL_LONG / EVAL_BOOLEAN / EVAL_OBJECT / EVAL_LIST 等
    Eval {
        script: String,
        keys:   Vec<Key>,
        args:   Vec<Value>,
    },

    /// 对应 Java RedisCommands.EVALSHA（evalAsync 里走 eval cache 时使用）
    EvalSha {
        sha:  String,
        keys: Vec<Key>,
        args: Vec<Value>,
    },

    /// 对应 Java CommandAsyncService 里动态构造的 "EVALSHA_RO"（readOnly + EVAL_SHA_RO_SUPPORTED）
    /// 失败时自动降级到 EvalSha
    EvalShaRo {
        sha:  String,
        keys: Vec<Key>,
        args: Vec<Value>,
    },

    // --------------------------------------------------------
    // SORT / SORT_RO
    // 对应 Java: RedisCommands.SORT / async() 里动态构造的 "SORT_RO"
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.SORT（普通写模式）
    Sort {
        key: Key,
    },

    /// 对应 Java async() 里动态构造的 "SORT_RO"（readOnly 模式，Redis 7.0+）
    /// 失败时自动降级到 Sort
    SortRo {
        key: Key,
    },

    // --------------------------------------------------------
    // Batch 同步命令（主从同步场景）
    // 对应 Java: RedisCommands.WAIT / RedisCommands.WAITAOF
    // --------------------------------------------------------

    /// 对应 Java RedisCommands.WAIT
    Wait {
        numreplicas: i64,
        timeout:     i64,
    },

    /// 对应 Java RedisCommands.WAITAOF
    WaitAof {
        numlocal:    i64,
        numreplicas: i64,
        timeout:     i64,
    },
}

impl RedisCommand {
    /// 对应 Java RedisExecutor.execute()，将命令分派到 fred 对应的 interface 方法。
    /// Pool 已封装连接获取、重试、超时；Options 由 CommandAsyncInner.build_options() 构建传入。
    pub async fn execute(self, pool: &Pool, options: &Options) -> anyhow::Result<Value> {
        let p = pool.with_options(options);
        match self {
            // ------------------------------------------------
            // String / key
            // ------------------------------------------------
            Self::Get { key } => {
                Ok(p.get(key).await?)
            }
            Self::Set { key, value, expiry, options: set_opts } => {
                Ok(p.set(key, value, expiry, set_opts, false).await?)
            }
            Self::Del { keys } => {
                Ok(p.del(keys).await?)
            }
            Self::Exists { keys } => {
                Ok(p.exists(keys).await?)
            }

            // ------------------------------------------------
            // 过期
            // ------------------------------------------------
            Self::Expire { key, seconds } => {
                Ok(p.expire(key, seconds, None).await?)
            }
            Self::Pexpire { key, milliseconds } => {
                Ok(p.pexpire(key, milliseconds, None).await?)
            }
            Self::Ttl { key } => {
                Ok(p.ttl(key).await?)
            }
            Self::Pttl { key } => {
                Ok(p.pttl(key).await?)
            }
            Self::Persist { key } => {
                Ok(p.persist(key).await?)
            }

            // ------------------------------------------------
            // Hash
            // ------------------------------------------------
            Self::HGet { key, field } => {
                Ok(p.hget(key, field).await?)
            }
            Self::HSet { key, field, value } => {
                Ok(p.hset(key, (field, value)).await?)
            }
            Self::HGetAll { key } => {
                Ok(p.hgetall(key).await?)
            }
            Self::HDel { key, fields } => {
                Ok(p.hdel(key, fields).await?)
            }
            Self::HExists { key, field } => {
                Ok(p.hexists(key, field).await?)
            }
            Self::HLen { key } => {
                Ok(p.hlen(key).await?)
            }
            Self::HIncrBy { key, field, delta } => {
                Ok(p.hincrby(key, field, delta).await?)
            }

            // ------------------------------------------------
            // Set
            // ------------------------------------------------
            Self::SAdd { key, members } => {
                Ok(p.sadd(key, members).await?)
            }
            Self::SRem { key, members } => {
                Ok(p.srem(key, members).await?)
            }
            Self::SMembers { key } => {
                Ok(p.smembers(key).await?)
            }
            Self::SIsMember { key, member } => {
                Ok(p.sismember(key, member).await?)
            }
            Self::SCard { key } => {
                Ok(p.scard(key).await?)
            }

            // ------------------------------------------------
            // Sorted Set
            // ------------------------------------------------
            Self::ZAdd { key, score, member, options: zadd_opts } => {
                Ok(p.zadd(key, zadd_opts, None, false, false, (score, member)).await?)
            }
            Self::ZRem { key, members } => {
                Ok(p.zrem(key, members).await?)
            }
            Self::ZScore { key, member } => {
                Ok(p.zscore(key, member).await?)
            }
            Self::ZCard { key } => {
                Ok(p.zcard(key).await?)
            }

            // ------------------------------------------------
            // Lua 脚本
            // ------------------------------------------------
            Self::Eval { script, keys, args } => {
                Ok(p.eval(script, keys, args).await?)
            }
            Self::EvalSha { sha, keys, args } => {
                Ok(p.evalsha(sha, keys, args).await?)
            }
            Self::EvalShaRo { sha, keys, args } => {
                // 对应 Java evalAsync() 里的 EVAL_SHA_RO_SUPPORTED 降级逻辑。
                // fred 没有 evalsha_ro 方法，走 pool.custom()。
                if EVAL_SHA_RO_SUPPORTED.load(Ordering::Relaxed) {
                    let slot = ClusterHash::FirstKey;
                    let mut cmd_args: Vec<Value> = Vec::with_capacity(1 + keys.len() + args.len());
                    cmd_args.push(sha.clone().into());
                    cmd_args.push((keys.len() as i64).into());
                    for k in keys.clone() { cmd_args.push(k.into()); }
                    for a in args.clone() { cmd_args.push(a); }
                    let cmd = CustomCommand::new_static("EVALSHA_RO", slot, false);
                    match pool.with_options(options).custom(cmd, cmd_args).await {
                        Ok(v) => return Ok(v),
                        Err(e) if e.details().contains("ERR unknown command") => {
                            EVAL_SHA_RO_SUPPORTED.store(false, Ordering::Relaxed);
                            Ok(p.evalsha(sha, keys, args).await?)
                        }
                        Err(e) => Err(anyhow::Error::from(e)),
                    }
                } else {
                    Ok(p.evalsha(sha, keys, args).await?)
                }
            }

            // ------------------------------------------------
            // SORT / SORT_RO
            // fred 的 sort/sort_ro 签名复杂，走 pool.custom()。
            // ------------------------------------------------
            Self::Sort { key } => {
                let cmd = CustomCommand::new_static("SORT", ClusterHash::FirstKey, false);
                let args: Vec<Value> = vec![key.into()];
                Ok(pool.with_options(options).custom(cmd, args).await?)
            }
            Self::SortRo { key } => {
                // 对应 Java async() 里的 SORT_RO_SUPPORTED 降级逻辑
                if SORT_RO_SUPPORTED.load(Ordering::Relaxed) {
                    let cmd = CustomCommand::new_static("SORT_RO", ClusterHash::FirstKey, false);
                    let sort_ro_args: Vec<Value> = vec![key.clone().into()];
                    match pool.with_options(options).custom(cmd, sort_ro_args).await {
                        Ok(v) => return Ok(v),
                        Err(e) if e.details().contains("ERR unknown command") => {
                            SORT_RO_SUPPORTED.store(false, Ordering::Relaxed);
                            let cmd = CustomCommand::new_static("SORT", ClusterHash::FirstKey, false);
                            let sort_args: Vec<Value> = vec![key.into()];
                            Ok(pool.with_options(options).custom(cmd, sort_args).await?)
                        }
                        Err(e) => Err(anyhow::Error::from(e)),
                    }
                } else {
                    let cmd = CustomCommand::new_static("SORT", ClusterHash::FirstKey, false);
                    let args: Vec<Value> = vec![key.into()];
                    Ok(pool.with_options(options).custom(cmd, args).await?)
                }
            }

            // ------------------------------------------------
            // Batch 同步
            // ------------------------------------------------
            Self::Wait { numreplicas, timeout } => {
                Ok(p.wait(numreplicas, timeout).await?)
            }
            Self::WaitAof { numlocal, numreplicas, timeout } => {
                // fred 没有 waitaof 方法，走 pool.custom()
                let cmd = CustomCommand::new_static("WAITAOF", ClusterHash::FirstKey, false);
                let args: Vec<Value> = vec![numlocal.into(), numreplicas.into(), timeout.into()];
                Ok(pool.with_options(options).custom(cmd, args).await?)
            }
        }
    }
}
