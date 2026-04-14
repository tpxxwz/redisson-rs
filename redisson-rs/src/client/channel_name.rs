use fred::types::Key;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

#[derive(Clone, Eq)]
pub struct ChannelName {
    /// 对应 Java: private final String str
    /// 内部同时承担 byte[] 的职责，因为 String 本身就是 UTF-8 字节
    str: String,
}

impl ChannelName {
    /// 对应 Java: public static final ChannelName TRACKING
    pub const TRACKING: &'static str = "__redis__:invalidate";

    /// 对应 Java: public static List<ChannelName> newList(ChannelName name)
    pub fn new_list(name: ChannelName) -> Vec<ChannelName> {
        vec![name]
    }

    /// 对应 Java: public static List<ChannelName> newList(String name)
    pub fn new_list_from_str(name: &str) -> Vec<ChannelName> {
        vec![Self::from(name)]
    }

    /// 对应 Java: public byte[] getName()
    pub fn get_name(&self) -> &[u8] {
        self.str.as_bytes()
    }

    /// 对应 Java: public String toString() —— 用 Display 代替
    /// 对应 Java: public int length()
    pub fn length(&self) -> usize {
        self.str.len()
    }

    /// 对应 Java: public char charAt(int index)
    pub fn char_at(&self, index: usize) -> Option<char> {
        self.str.chars().nth(index)
    }

    /// 对应 Java: public CharSequence subSequence(int start, int end)
    pub fn sub_sequence(&self, start: usize, end: usize) -> &str {
        &self.str[start..end]
    }

    /// 对应 Java: public boolean isKeyspace()
    pub fn is_keyspace(&self) -> bool {
        self.str.starts_with("__keyspace") || self.str.starts_with("__keyevent")
    }

    /// 对应 Java: public boolean isTracking()
    pub fn is_tracking(&self) -> bool {
        self.str == Self::TRACKING
    }

    /// 用 DefaultHasher 计算 channel 名的 u64 hash，供分片/取模等场景使用。
    pub fn hash_u64(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// 对应 Java: public ChannelName(String name)
impl From<&str> for ChannelName {
    fn from(name: &str) -> Self {
        Self { str: name.to_owned() }
    }
}

impl From<String> for ChannelName {
    fn from(name: String) -> Self {
        Self { str: name }
    }
}

pub struct MultipleChannelNames {
    names: Vec<ChannelName>,
}

impl From<ChannelName> for MultipleChannelNames {
    fn from(name: ChannelName) -> Self {
        Self { names: vec![name] }
    }
}

impl From<Vec<ChannelName>> for MultipleChannelNames {
    fn from(names: Vec<ChannelName>) -> Self {
        Self { names }
    }
}

impl MultipleChannelNames {
    pub fn into_vec(self) -> Vec<ChannelName> {
        self.names
    }
}

impl From<ChannelName> for Key {
    fn from(name: ChannelName) -> Self {
        Key::from(name.str)
    }
}

impl From<&ChannelName> for Key {
    fn from(name: &ChannelName) -> Self {
        Key::from(name.str.as_str())
    }
}

/// 对应 Java: public String toString()
impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.str)
    }
}

/// 对应 Java: hashCode —— 基于字节内容，和 Java Arrays.hashCode(name) 语义一致
impl Hash for ChannelName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.str.as_bytes().hash(state);
    }
}

/// 对应 Java: equals
impl PartialEq for ChannelName {
    fn eq(&self, other: &Self) -> bool {
        self.str.as_bytes() == other.str.as_bytes()
    }
}

/// 对应 Java: equals 中 obj instanceof CharSequence 分支
impl PartialEq<str> for ChannelName {
    fn eq(&self, other: &str) -> bool {
        self.str == other
    }
}

impl PartialEq<String> for ChannelName {
    fn eq(&self, other: &String) -> bool {
        &self.str == other
    }
}

/// 让 ChannelName 直接当 &str 用，替代 CharSequence 的大部分场景
impl Deref for ChannelName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.str
    }
}