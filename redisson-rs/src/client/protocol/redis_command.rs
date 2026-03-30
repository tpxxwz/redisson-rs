use crate::client::protocol::convertor::Convertor;
use std::marker::PhantomData;

// ============================================================
// RedisCommand<R> — 对应 Java org.redisson.client.protocol.RedisCommand<R>
// ============================================================

pub struct RedisCommand<R: 'static> {
    /// 对应 Java RedisCommand.name
    pub(crate) name: &'static str,
    /// 对应 Java RedisCommand.subName
    pub(crate) sub_name: Option<&'static str>,
    /// 对应 Java RedisCommand.convertor
    /// None 时使用 fred 的 FromValue 直接转换（相当于 EmptyConvertor）
    pub(crate) convertor: Option<&'static dyn Convertor<R>>,
    _phantom: PhantomData<fn() -> R>,
}

impl<R: 'static> RedisCommand<R> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            sub_name: None,
            convertor: None,
            _phantom: PhantomData,
        }
    }

    pub const fn new_sub(name: &'static str, sub_name: &'static str) -> Self {
        Self {
            name,
            sub_name: Some(sub_name),
            convertor: None,
            _phantom: PhantomData,
        }
    }

    pub const fn new_with_convertor(
        name: &'static str,
        convertor: &'static dyn Convertor<R>,
    ) -> Self {
        Self {
            name,
            sub_name: None,
            convertor: Some(convertor),
            _phantom: PhantomData,
        }
    }

    pub const fn new_sub_with_convertor(
        name: &'static str,
        sub_name: &'static str,
        convertor: &'static dyn Convertor<R>,
    ) -> Self {
        Self {
            name,
            sub_name: Some(sub_name),
            convertor: Some(convertor),
            _phantom: PhantomData,
        }
    }
}
