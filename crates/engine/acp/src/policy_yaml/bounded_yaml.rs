//! Charge expanded YAML values before retaining them, including alias copies.

use std::fmt;

use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde_yaml::{
    value::{Tag, TaggedValue},
    Mapping, Value,
};

struct Budget {
    nodes: usize,
    bytes: usize,
}

impl Budget {
    fn bytes<E: de::Error>(&mut self, amount: usize) -> Result<(), E> {
        self.bytes = self
            .bytes
            .checked_sub(amount)
            .ok_or_else(|| E::custom("policy YAML expansion exceeds byte limit"))?;
        Ok(())
    }
}

struct BoundedValue<'a>(&'a mut Budget);

impl<'de> DeserializeSeed<'de> for BoundedValue<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        self.0.nodes = self
            .0
            .nodes
            .checked_sub(1)
            .ok_or_else(|| de::Error::custom("policy YAML expansion exceeds node limit"))?;
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for BoundedValue<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded YAML value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.0.bytes(value.len())?;
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        self.0.bytes(value.len())?;
        Ok(Value::String(value))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        self.deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = access.next_element_seed(BoundedValue(&mut *self.0))? {
            values.push(value);
        }
        Ok(Value::Sequence(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values = Mapping::new();
        while let Some(key) = access.next_key_seed(BoundedValue(&mut *self.0))? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(match key {
                    Value::String(key) => format!("duplicate entry with key \"{key}\""),
                    _ => "duplicate entry with non-string key".into(),
                }));
            }
            let value = access.next_value_seed(BoundedValue(&mut *self.0))?;
            values.insert(key, value);
        }
        Ok(Value::Mapping(values))
    }

    fn visit_enum<A: EnumAccess<'de>>(self, access: A) -> Result<Value, A::Error> {
        let (tag, value) = access.variant::<String>()?;
        if tag.is_empty() {
            return Err(de::Error::custom("empty YAML tag is not allowed"));
        }
        self.0.bytes(tag.len())?;
        let value = value.newtype_variant_seed(BoundedValue(self.0))?;
        Ok(Value::Tagged(Box::new(TaggedValue {
            tag: Tag::new(tag),
            value,
        })))
    }
}

pub(super) fn parse(input: &str) -> Result<Value, serde_yaml::Error> {
    if input.len() > 64 * 1024 {
        return Err(de::Error::custom("policy definition exceeds 64 KiB"));
    }
    // The wire limit bounds tokens, not alias expansion. These independent
    // budgets bound both large scalar copies and collections of tiny values.
    let mut budget = Budget {
        nodes: 64 * 1024,
        bytes: 1024 * 1024,
    };
    BoundedValue(&mut budget).deserialize(serde_yaml::Deserializer::from_str(input))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_yaml::Value;

    #[test]
    fn bounded_values_preserve_yaml_types_and_tags() {
        for input in [
            "",
            "[null, true, -1, 42, 1.5, text]",
            "{name: value, list: [a, b]}",
            "!Thing {key: value}",
            "!Thing [1, 2]",
            "!Thing text",
        ] {
            assert_eq!(
                parse(input).unwrap(),
                serde_yaml::from_str::<Value>(input).unwrap()
            );
        }
    }

    #[test]
    fn duplicate_keys_and_multiple_documents_are_rejected() {
        assert!(parse("name: a\nname: b").is_err());
        assert!(parse("name: a\n---\nname: b").is_err());
    }
}
