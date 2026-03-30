use bytes::Bytes;
use fred::prelude::Value;
use super::convertor::{
    BooleanAmountReplayConvertor, BooleanNotNullReplayConvertor,
    BooleanNullSafeReplayConvertor, BooleanReplayConvertor,
};
use super::redis_command::RedisCommand;

// ============================================================
// RedisCommands — 对应 Java org.redisson.client.protocol.RedisCommands
// ============================================================

// ── 通用命令 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.EXISTS (BooleanAmountReplayConvertor)
pub const EXISTS: RedisCommand<bool> =
    RedisCommand::new_with_convertor("EXISTS", &BooleanAmountReplayConvertor);
/// 对应 Java RedisCommands.DEL (Long)
pub const DEL: RedisCommand<i64> = RedisCommand::new("DEL");
/// 对应 Java RedisCommands.UNLINK (Long)
pub const UNLINK: RedisCommand<i64> = RedisCommand::new("UNLINK");
/// 对应 Java RedisCommands.TOUCH (BooleanReplayConvertor)
pub const TOUCH: RedisCommand<bool> =
    RedisCommand::new_with_convertor("TOUCH", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.RENAME (Void)
pub const RENAME: RedisCommand<()> = RedisCommand::new("RENAME");
/// 对应 Java RedisCommands.RENAMENX (BooleanReplayConvertor)
pub const RENAMENX: RedisCommand<bool> =
    RedisCommand::new_with_convertor("RENAMENX", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.MOVE (BooleanReplayConvertor)
pub const MOVE: RedisCommand<bool> =
    RedisCommand::new_with_convertor("MOVE", &BooleanReplayConvertor);

// ── 对象操作命令 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.OBJECT_IDLETIME (Long)
pub const OBJECT_IDLETIME: RedisCommand<i64> = RedisCommand::new_sub("OBJECT", "IDLETIME");
/// 对应 Java RedisCommands.OBJECT_REFCOUNT (Integer)
pub const OBJECT_REFCOUNT: RedisCommand<i32> = RedisCommand::new_sub("OBJECT", "REFCOUNT");
/// 对应 Java RedisCommands.OBJECT_FREQ (Integer)
pub const OBJECT_FREQ: RedisCommand<i32> = RedisCommand::new_sub("OBJECT", "FREQ");
/// 对应 Java RedisCommands.OBJECT_ENCODING (在调用方转换为 ObjectEncoding)
pub const OBJECT_ENCODING: RedisCommand<String> = RedisCommand::new_sub("OBJECT", "ENCODING");
/// 对应 Java RedisCommands.MEMORY_USAGE (Long)
pub const MEMORY_USAGE: RedisCommand<i64> = RedisCommand::new_sub("MEMORY", "USAGE");
/// 对应 Java RedisCommands.DUMP
pub const DUMP: RedisCommand<Bytes> = RedisCommand::new("DUMP");
/// 对应 Java RedisCommands.RESTORE (Void)
pub const RESTORE: RedisCommand<()> = RedisCommand::new("RESTORE");
/// 对应 Java RedisCommands.MIGRATE (Void)
pub const MIGRATE: RedisCommand<()> = RedisCommand::new("MIGRATE");
/// 对应 Java RedisCommands.COPY (BooleanReplayConvertor)
pub const COPY: RedisCommand<bool> =
    RedisCommand::new_with_convertor("COPY", &BooleanReplayConvertor);

// ── 过期命令 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.PTTL (Long)
pub const PTTL: RedisCommand<i64> = RedisCommand::new("PTTL");
/// 对应 Java RedisCommands.PEXPIRE (BooleanReplayConvertor)
pub const PEXPIRE: RedisCommand<bool> =
    RedisCommand::new_with_convertor("PEXPIRE", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.PEXPIREAT (BooleanReplayConvertor)
pub const PEXPIREAT: RedisCommand<bool> =
    RedisCommand::new_with_convertor("PEXPIREAT", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.PEXPIRETIME (Long)
pub const PEXPIRETIME: RedisCommand<i64> = RedisCommand::new("PEXPIRETIME");
/// 对应 Java RedisCommands.PERSIST (BooleanReplayConvertor)
pub const PERSIST: RedisCommand<bool> =
    RedisCommand::new_with_convertor("PERSIST", &BooleanReplayConvertor);

// ── Hash 命令 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.HEXISTS (BooleanReplayConvertor)
pub const HEXISTS: RedisCommand<bool> =
    RedisCommand::new_with_convertor("HEXISTS", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.HGET
pub const HGET: RedisCommand<Option<String>> = RedisCommand::new("HGET");
/// 对应 Java RedisCommands.HSET (BooleanReplayConvertor)
pub const HSET: RedisCommand<bool> =
    RedisCommand::new_with_convertor("HSET", &BooleanReplayConvertor);
/// 对应 Java RedisCommands.HDEL (Long)
pub const HDEL: RedisCommand<i64> = RedisCommand::new("HDEL");

// ── String 命令 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.GET
pub const GET: RedisCommand<Option<String>> = RedisCommand::new("GET");
/// 对应 Java RedisCommands.SET (Void)
pub const SET: RedisCommand<()> = RedisCommand::new("SET");
/// 对应 Java RedisCommands.STRLEN (Long)
pub const STRLEN: RedisCommand<i64> = RedisCommand::new("STRLEN");

// ── DEL 变体 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.DEL_BOOL (BooleanNullSafeReplayConvertor)
pub const DEL_BOOL: RedisCommand<bool> =
    RedisCommand::new_with_convertor("DEL", &BooleanNullSafeReplayConvertor);

// ── EXISTS 变体 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.NOT_EXISTS (BooleanNotNullReplayConvertor)
pub const NOT_EXISTS: RedisCommand<bool> =
    RedisCommand::new_with_convertor("EXISTS", &BooleanNotNullReplayConvertor);

// ── UNLINK 变体 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.UNLINK_BOOL (BooleanNullSafeReplayConvertor)
pub const UNLINK_BOOL: RedisCommand<bool> =
    RedisCommand::new_with_convertor("UNLINK", &BooleanNullSafeReplayConvertor);

// ── EVAL 系列 ────────────────────────────────────────────────────

/// 对应 Java RedisCommands.EVAL_LONG (Long)
pub const EVAL_LONG: RedisCommand<i64> = RedisCommand::new("EVAL");
/// 对应 Java RedisCommands.EVAL_OBJECT (Object / raw Value)
pub const EVAL_OBJECT: RedisCommand<Value> = RedisCommand::new("EVAL");
/// 对应 Java RedisCommands.EVAL_BOOLEAN (BooleanReplayConvertor)
pub const EVAL_BOOLEAN: RedisCommand<bool> =
    RedisCommand::new_with_convertor("EVAL", &BooleanReplayConvertor);
/// 对应 Java EVALSHA 变体，命令名为 EVALSHA
pub const EVALSHA_OBJECT: RedisCommand<Value> = RedisCommand::new("EVALSHA");
/// SCRIPT LOAD script — 完整的 SCRIPT LOAD 调用
/// 注意：SCRIPT LOAD 是两个词，args = ["LOAD", script]
pub const SCRIPT_LOAD: (&'static str, &'static str) = ("SCRIPT", "LOAD");
