use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub(super) fn parse_i64_value<E: serde::de::Error>(value: Value) -> Result<i64, E> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| E::custom("ID must fit PostgreSQL BIGINT")),
        Value::String(text) => text
            .parse::<i64>()
            .map_err(|_| E::custom("ID must be a decimal PostgreSQL BIGINT")),
        _ => Err(E::custom("ID must be a decimal PostgreSQL BIGINT")),
    }
}
pub(super) fn deserialize_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    parse_i64_value(Value::deserialize(deserializer)?)
}
pub(super) fn deserialize_optional_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        Ok(None)
    } else {
        parse_i64_value(value).map(Some)
    }
}
