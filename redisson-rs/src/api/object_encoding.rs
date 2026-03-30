// ============================================================
// ObjectEncoding — 对应 Java org.redisson.api.ObjectEncoding
// ============================================================

/// Internal encoding of Redis object
/// 对应 Java ObjectEncoding enum
/// 参考: https://redis.io/docs/latest/commands/object-encoding/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectEncoding {
    /// Normal string encoding.
    /// 普通 raw 字符串编码
    RAW,

    /// Strings representing integers in a 64-bit signed interval.
    /// 64 位有符号整数范围内的整数字符串
    INT,

    /// Strings with lengths up to the hardcoded limit of OBJ_ENCODING_EMBSTR_SIZE_LIMIT or 44 bytes.
    /// 长度不超过 44 字节的短字符串
    EMBSTR,

    /// An old list encoding. No longer used.
    /// 旧的列表编码，已不再使用
    LINKEDLIST,

    /// A space-efficient encoding used for small lists. Redis <= 6.2
    /// 小列表的节省空间编码，Redis 6.2 及以下版本
    ZIPLIST,

    /// A space-efficient encoding used for small lists. Redis >= 7.0
    /// 小列表的节省空间编码，Redis 7.0 及以上版本
    LISTPACK,

    /// Encoded as linkedlist of ziplists or listpacks.
    /// 由 ziplist 或 listpack 组成的链表
    QUICKLIST,

    /// Normal set encoding.
    /// 普通集合编码
    HASHTABLE,

    /// Small sets composed solely of integers encoding.
    /// 仅包含整数的小集合编码
    INTSET,

    /// An old hash encoding. No longer used.
    /// 旧的哈希编码，已不再使用
    ZIPMAP,

    /// Normal sorted set encoding.
    /// 普通有序集合编码
    SKIPLIST,

    /// Encoded as a radix tree of listpacks.
    /// 由 listpack 组成的基数树（Stream 类型）
    STREAM,

    /// Key is not exist.
    /// Key 不存在
    NULL,

    /// This means redis support new type and this Enum not defined.
    /// Redis 支持的新类型但枚举未定义
    UNKNOWN,
}

impl ObjectEncoding {
    /// 对应 Java: public String getType()
    /// Returns the type string as returned by Redis OBJECT ENCODING command.
    pub fn get_type(&self) -> &'static str {
        match self {
            Self::RAW => "raw",
            Self::INT => "int",
            Self::EMBSTR => "embstr",
            Self::LINKEDLIST => "linkedlist",
            Self::ZIPLIST => "ziplist",
            Self::LISTPACK => "listpack",
            Self::QUICKLIST => "quicklist",
            Self::HASHTABLE => "hashtable",
            Self::INTSET => "intset",
            Self::ZIPMAP => "zipmap",
            Self::SKIPLIST => "skiplist",
            Self::STREAM => "stream",
            Self::NULL => "nonexistence",
            Self::UNKNOWN => "unknown",
        }
    }

    /// 对应 Java: public static ObjectEncoding valueOfEncoding(Object object)
    /// Parse from Redis OBJECT ENCODING response.
    /// Returns NULL if input is None, UNKNOWN if type is unrecognized.
    pub fn value_of_encoding(object: Option<&str>) -> Self {
        match object {
            None => Self::NULL,
            Some(value) => match value.to_lowercase().as_str() {
                "raw" => Self::RAW,
                "int" => Self::INT,
                "embstr" => Self::EMBSTR,
                "linkedlist" => Self::LINKEDLIST,
                "ziplist" => Self::ZIPLIST,
                "listpack" => Self::LISTPACK,
                "quicklist" => Self::QUICKLIST,
                "hashtable" => Self::HASHTABLE,
                "intset" => Self::INTSET,
                "zipmap" => Self::ZIPMAP,
                "skiplist" => Self::SKIPLIST,
                "stream" => Self::STREAM,
                "nonexistence" => Self::NULL,
                _ => Self::UNKNOWN,
            },
        }
    }
}
