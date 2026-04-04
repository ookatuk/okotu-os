use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt;
use base64::Engine;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{MapAccess, Visitor};
use crate::version::types::HashVariant;

impl<'a> Serialize for HashVariant<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("HashVariant", 2)?;

        state.serialize_field("algo", &self.algo())?;
        state.serialize_field("hash", &self.hash())?;

        state.end()
    }
}

impl<'de> Deserialize<'de> for HashVariant<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field { Algo, Hash }

        struct HashVariantVisitor;

        impl<'de> Visitor<'de> for HashVariantVisitor {
            type Value = HashVariant<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct HashVariant")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut algo: Option<String> = None;
                let mut hash_hex: Option<String> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Algo => algo = Some(map.next_value()?),
                        Field::Hash => hash_hex = Some(map.next_value()?),
                    }
                }

                let algo_str = algo.ok_or_else(|| de::Error::missing_field("algo"))?;
                let hex_str = hash_hex.ok_or_else(|| de::Error::missing_field("hash"))?;

                let hash_bytes = base64::prelude::BASE64_URL_SAFE.decode(&hex_str).map_err(|_| {
                    de::Error::invalid_value(de::Unexpected::Str(&hex_str), &"valid base64 string")
                })?;

                HashVariant::from_parts(&algo_str, Cow::Owned(hash_bytes))
                    .map_err(|e| de::Error::custom(e))
            }
        }

        const FIELDS: &[&str] = &["algo", "hash"];
        deserializer.deserialize_struct("HashVariant", FIELDS, HashVariantVisitor)
    }
}