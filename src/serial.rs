//! Serde integration support.

use std::fmt;

use serde::de::{Error, Unexpected, Visitor};
use serde::*;

fn i64_to_u64<'d, V: Visitor<'d>, E: Error>(v: V, n: i64) -> Result<V::Value, E> {
    if n >= 0 {
        v.visit_u64(n as u64)
    } else {
        Err(E::invalid_value(Unexpected::Signed(n), &v))
    }
}

/// Ignore deserialization errors and revert to default.
pub fn ignore_errors<'d, T: Deserialize<'d> + Default, D: Deserializer<'d>>(
    d: D,
) -> Result<T, D::Error> {
    use serde_json::Value;

    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).ok().unwrap_or_default())
}

/// Deserialize a maybe-string ID into a u64.
pub fn deserialize_id<'d, D: Deserializer<'d>>(d: D) -> Result<u64, D::Error> {
    struct IdVisitor;
    impl<'d> Visitor<'d> for IdVisitor {
        type Value = u64;

        fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            write!(fmt, "a u64 or parseable string")
        }

        fn visit_i64<E: Error>(self, v: i64) -> Result<u64, E> {
            i64_to_u64(self, v)
        }

        fn visit_u64<E: Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<u64, E> {
            v.parse::<u64>()
                .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
        }
    }

    d.deserialize_any(IdVisitor)
}

/// Deserialize a maybe-string discriminator into a u16.
/// Also enforces 0 <= N <= 9999.
#[allow(unused_comparisons)]
pub fn deserialize_discrim_opt<'d, D: Deserializer<'d>>(d: D) -> Result<Option<u16>, D::Error> {
    macro_rules! check {
        ($self:ident, $v:ident, $wrong:expr) => {
            if $v >= 0 && $v <= 9999 {
                Ok(Some($v as u16))
            } else {
                Err(E::invalid_value($wrong, &$self))
            }
        };
    }

    struct DiscrimVisitor;
    impl<'d> Visitor<'d> for DiscrimVisitor {
        type Value = Option<u16>;

        fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            write!(fmt, "a u16 in [0, 9999] or parseable string")
        }

        fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
            check!(self, v, Unexpected::Signed(v))
        }

        fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
            check!(self, v, Unexpected::Unsigned(v))
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
            v.parse::<u16>()
                .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
                .and_then(|v| self.visit_u16(v))
        }
    }

    d.deserialize_any(DiscrimVisitor)
}

pub fn deserialize_discrim<'d, D: Deserializer<'d>>(d: D) -> Result<u16, D::Error> {
    match deserialize_discrim_opt(d) {
        Ok(Some(result)) => Ok(result),
        Err(e) => Err(e),
        Ok(None) => Err(D::Error::missing_field("discriminator")),
    }
}

/// Special support for the oddly complex `ReactionEmoji`.
pub mod reaction_emoji {
    use super::*;
    use crate::model::{EmojiId, ReactionEmoji};

    #[derive(Serialize)]
    struct EmojiSer<'s> {
        name: &'s str,
        id: Option<EmojiId>,
        animated: Option<bool>,
    }

    #[derive(Deserialize)]
    struct EmojiDe {
        name: String,
        id: Option<EmojiId>,
        animated: Option<bool>,
    }

    pub fn serialize<S: Serializer>(v: &ReactionEmoji, s: S) -> Result<S::Ok, S::Error> {
        (match *v {
            ReactionEmoji::Unicode { ref name } => EmojiSer {
                name: name,
                id: None,
                animated: None,
            },
            ReactionEmoji::Custom {
                ref name,
                id,
                animated,
            } => EmojiSer {
                id: Some(id),
                name: name,
                animated: Some(animated),
            },
        })
        .serialize(s)
    }

    pub fn deserialize<'d, D: Deserializer<'d>>(d: D) -> Result<ReactionEmoji, D::Error> {
        Ok(match EmojiDe::deserialize(d)? {
            EmojiDe {
                name,
                id: None,
                animated: None,
            } => ReactionEmoji::Unicode { name },
            EmojiDe {
                name,
                id: Some(id),
                animated: Some(animated),
            } => ReactionEmoji::Custom { name, id, animated },
            _ => {
                return Err(Error::custom(
                    "unexpected combination of fields `id` and `animated`",
                ))
            }
        })
    }
}

/// Make sure a field holds a certain numeric value, or fail otherwise.
#[derive(Debug, Clone)]
pub struct Eq<const N: u64>;

impl<'de, const N: u64> Deserialize<'de> for Eq<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NumberVisitor<const N: u64>;

        impl<'d, const N: u64> Visitor<'d> for NumberVisitor<N> {
            type Value = u64;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "the number {}", N)
            }

            fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Unsigned(v as u64), &self))
                }
            }

            fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Unsigned(v as u64), &self))
                }
            }

            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Unsigned(v as u64), &self))
                }
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Unsigned(v), &self))
                }
            }

            fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // n can't be negative so no checks required
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Signed(v as i64), &self))
                }
            }

            fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // n can't be negative so no checks required
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Signed(v as i64), &self))
                }
            }

            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // n can't be negative so no checks required
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Signed(v as i64), &self))
                }
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // n can't be negative so no checks required
                if v as u64 == N {
                    Ok(v as u64)
                } else {
                    Err(E::invalid_value(Unexpected::Signed(v), &self))
                }
            }
        }

        deserializer.deserialize_any(NumberVisitor)?;
        Ok(Self)
    }
}

impl<const N: u64> Serialize for Eq<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(N)
    }
}
