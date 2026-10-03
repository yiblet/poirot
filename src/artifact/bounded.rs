//! Decode budgets applied before collection reservations and owned string copies.
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use std::{cell::Cell, fmt};

pub(super) struct Budget {
    values: Cell<usize>,
    bytes: Cell<usize>,
    depth: usize,
}
impl Budget {
    pub fn new(values: usize, bytes: usize, depth: usize) -> Self {
        Self {
            values: Cell::new(values),
            bytes: Cell::new(bytes),
            depth,
        }
    }
    fn charge<E: de::Error>(&self, bytes: usize) -> Result<(), E> {
        let remaining = self
            .bytes
            .get()
            .checked_sub(bytes)
            .ok_or_else(|| E::custom("snapshot decoded byte limit"))?;
        self.bytes.set(remaining);
        Ok(())
    }
    fn enter<E: de::Error>(&self, depth: usize) -> Result<(), E> {
        if depth > self.depth {
            return Err(E::custom("snapshot decode depth limit"));
        }
        let remaining = self
            .values
            .get()
            .checked_sub(1)
            .ok_or_else(|| E::custom("snapshot decoded value limit"))?;
        self.values.set(remaining);
        Ok(())
    }
    fn sequence<E: de::Error>(&self, len: Option<usize>) -> Result<(), E> {
        if len.is_some_and(|len| len > self.values.get()) {
            return Err(E::custom(
                "snapshot collection length exceeds decode budget",
            ));
        }
        Ok(())
    }
}

pub(super) struct Decoder<'a, D> {
    pub inner: D,
    pub budget: &'a Budget,
    pub depth: usize,
}
struct Guard<'a, V> {
    inner: V,
    budget: &'a Budget,
    depth: usize,
}
struct Seed<'a, S> {
    inner: S,
    budget: &'a Budget,
    depth: usize,
}
impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<'_, S> {
    type Value = S::Value;
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
        fn $name<V: Visitor<'de>>(self, $($($arg: $ty,)*)? visitor: V) -> Result<V::Value, Self::Error> {
            self.budget.enter(self.depth)?;
            self.inner.$name($($($arg,)*)? Guard { inner: visitor, budget: self.budget, depth: self.depth })
        }
    )*};
}
impl<'de, D: de::Deserializer<'de>> de::Deserializer<'de> for Decoder<'_, D> {
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
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}
macro_rules! scalar {
    ($($name:ident($ty:ty);)*) => {$ (
        fn $name<E: de::Error>(self, v: $ty) -> Result<Self::Value, E> { self.inner.$name(v) }
    )*};
}
impl<'de, V: Visitor<'de>> Visitor<'de> for Guard<'_, V> {
    type Value = V::Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.expecting(f)
    }
    scalar! { visit_bool(bool); visit_i8(i8); visit_i16(i16); visit_i32(i32); visit_i64(i64);
    visit_i128(i128); visit_u8(u8); visit_u16(u16); visit_u32(u32); visit_u64(u64);
    visit_u128(u128); visit_f32(f32); visit_f64(f64); visit_char(char); }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_str(v)
    }
    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_borrowed_str(v)
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_string(v)
    }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_bytes(v)
    }
    fn visit_borrowed_bytes<E: de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_borrowed_bytes(v)
    }
    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        self.budget.charge(v.len())?;
        self.inner.visit_byte_buf(v)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_some<D: de::Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Decoder {
            inner,
            budget: self.budget,
            depth: self.depth + 1,
        })
    }
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
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.sequence(inner.size_hint())?;
        self.inner.visit_seq(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.sequence(inner.size_hint())?;
        self.inner.visit_map(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }
    fn visit_enum<A: EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Guard {
            inner,
            budget: self.budget,
            depth: self.depth,
        })
    }
}
impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Guard<'_, A> {
    type Error = A::Error;
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
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
impl<'de, A: MapAccess<'de>> MapAccess<'de> for Guard<'_, A> {
    type Error = A::Error;
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
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
impl<'de, 'a, A: EnumAccess<'de>> EnumAccess<'de> for Guard<'a, A> {
    type Error = A::Error;
    type Variant = Guard<'a, A::Variant>;
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
    type Error = A::Error;
    fn unit_variant(self) -> Result<(), Self::Error> {
        self.inner.unit_variant()
    }
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
