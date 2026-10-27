//! Budgeted deserializer applied before collection reservations and owned string copies.
//!
//! These wrappers run once per decoded value, so every method is `#[inline]`; otherwise
//! codegen-unit splits can leave each wrapper as an out-of-line call around the codec.
use super::budget::Budget;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use std::fmt;

/// Deserializer applying cumulative limits before allocating payloads.
pub(in crate::eval::artifact) struct Decoder<'a, D> {
    pub inner: D,
    pub budget: &'a Budget,
    pub depth: usize,
}

/// Visitor that charges each decoded value before forwarding it.
struct Guard<'a, V> {
    inner: V,
    budget: &'a Budget,
    depth: usize,
}

/// Nested seed retaining the shared decode budget.
struct Seed<'a, S> {
    inner: S,
    budget: &'a Budget,
    depth: usize,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<'_, S> {
    /// Decoded value forwarded to the original visitor.
    type Value = S::Value;

    #[inline]
    fn deserialize<D: de::Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.deserialize(Decoder {
            inner,
            budget: self.budget,
            depth: self.depth + 1,
        })
    }
}

macro_rules! decode {
    ($($name:ident $(($($arg:ident : $ty:ty),*))?;)*) => {$ (
        #[inline]
        fn $name<V: Visitor<'de>>(self, $($($arg: $ty,)*)? visitor: V) -> Result<V::Value, Self::Error> {
            self.budget.enter(self.depth)?;
            self.inner.$name($($($arg,)*)? Guard { inner: visitor, budget: self.budget, depth: self.depth })
        }
    )*};
}

impl<'de, D: de::Deserializer<'de>> de::Deserializer<'de> for Decoder<'_, D> {
    /// Underlying codec error used for budget refusals.
    type Error = D::Error;

    decode! {
        deserialize_any; deserialize_bool; deserialize_i8; deserialize_i16; deserialize_i32;
        deserialize_i64; deserialize_i128; deserialize_u8; deserialize_u16; deserialize_u32;
        deserialize_u64; deserialize_u128; deserialize_f32; deserialize_f64; deserialize_char;
        deserialize_str; deserialize_string; deserialize_bytes; deserialize_byte_buf;
        deserialize_option; deserialize_unit; deserialize_unit_struct(name: &'static str);
        deserialize_newtype_struct(name: &'static str); deserialize_seq;
        deserialize_tuple(len: usize); deserialize_tuple_struct(name: &'static str, len: usize);
        deserialize_map; deserialize_struct(name: &'static str, fields: &'static [&'static str]);
        deserialize_enum(name: &'static str, variants: &'static [&'static str]);
        deserialize_identifier; deserialize_ignored_any;
    }

    #[inline]
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

macro_rules! scalar {
    ($($name:ident($ty:ty);)*) => {$ (
        #[inline]
        fn $name<E: de::Error>(self, v: $ty) -> Result<Self::Value, E> { self.inner.$name(v) }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Guard<'_, V> {
    /// Decoded value forwarded to the original visitor.
    type Value = V::Value;

    #[inline]
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.expecting(f)
    }

    scalar! { visit_bool(bool); visit_i8(i8); visit_i16(i16); visit_i32(i32); visit_i64(i64);
    visit_i128(i128); visit_u8(u8); visit_u16(u16); visit_u32(u32); visit_u64(u64);
    visit_u128(u128); visit_f32(f32); visit_f64(f64); visit_char(char); }

    #[inline]
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_str(v)
    }

    #[inline]
    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_borrowed_str(v)
    }

    #[inline]
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_string(v)
    }

    #[inline]
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_bytes(v)
    }

    #[inline]
    fn visit_borrowed_bytes<E: de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_borrowed_bytes(v)
    }

    #[inline]
    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_byte_buf(v)
    }

    #[inline]
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }

    #[inline]
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }

    #[inline]
    fn visit_some<D: de::Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Decoder {
            inner,
            budget: self.budget,
            depth: self.depth + 1,
        })
    }

    #[inline]
    fn visit_newtype_struct<D: de::Deserializer<'de>>(
        self,
        inner: D,
    ) -> Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(Decoder {
            inner,
            budget: self.budget,
            depth: self.depth + 1,
        })
    }

    #[inline]
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.sequence(inner.size_hint())?;
        self.inner.visit_seq(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    #[inline]
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.sequence(inner.size_hint())?;
        self.inner.visit_map(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    #[inline]
    fn visit_enum<A: EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Guard<'_, A> {
    /// Underlying codec error used for budget refusals.
    type Error = A::Error;

    #[inline]
    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        inner: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        self.inner.next_element_seed(Seed {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    // Never expose an untrusted reservation hint to Vec/Cow visitors.
    #[inline]
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Guard<'_, A> {
    /// Underlying codec error used for budget refusals.
    type Error = A::Error;

    #[inline]
    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        inner: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        self.inner.next_key_seed(Seed {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    #[inline]
    fn next_value_seed<S: DeserializeSeed<'de>>(
        &mut self,
        inner: S,
    ) -> Result<S::Value, Self::Error> {
        self.inner.next_value_seed(Seed {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    #[inline]
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

impl<'de, 'a, A: EnumAccess<'de>> EnumAccess<'de> for Guard<'a, A> {
    /// Underlying codec error used for budget refusals.
    type Error = A::Error;

    /// Bounded enum-variant decoder.
    type Variant = Guard<'a, A::Variant>;

    #[inline]
    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        inner: S,
    ) -> Result<(S::Value, Self::Variant), Self::Error> {
        let (value, inner) = self.inner.variant_seed(Seed {
            inner,
            budget: self.budget,
            depth: self.depth,
        })?;
        Ok((
            value,
            Guard {
                inner,
                budget: self.budget,
                depth: self.depth,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Guard<'_, A> {
    /// Underlying codec error used for budget refusals.
    type Error = A::Error;

    #[inline]
    fn unit_variant(self) -> Result<(), Self::Error> {
        self.inner.unit_variant()
    }

    #[inline]
    fn newtype_variant_seed<S: DeserializeSeed<'de>>(
        self,
        inner: S,
    ) -> Result<S::Value, Self::Error> {
        self.inner.newtype_variant_seed(Seed {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }

    #[inline]
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, inner: V) -> Result<V::Value, Self::Error> {
        self.budget.enter(self.depth + 1)?;
        self.inner.tuple_variant(
            len,
            Guard {
                inner,
                budget: self.budget,
                depth: self.depth + 1,
            },
        )
    }

    #[inline]
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        inner: V,
    ) -> Result<V::Value, Self::Error> {
        self.budget.enter(self.depth + 1)?;
        self.inner.struct_variant(
            fields,
            Guard {
                inner,
                budget: self.budget,
                depth: self.depth + 1,
            },
        )
    }
}
