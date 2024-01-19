//! A Koto language module for working with JSON data

use koto_runtime::prelude::*;
use koto_serialize::SerializableValue;
use serde_json::Value as JsonValue;

pub fn json_value_to_koto_value(value: &serde_json::Value) -> Result<Value, String> {
    let result = match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => match n.as_i64() {
            Some(n64) => Value::Number(n64.into()),
            None => match n.as_f64() {
                Some(n64) => Value::Number(n64.into()),
                None => return Err(format!("Number is out of range: {n}")),
            },
        },
        JsonValue::String(s) => Value::Str(s.as_str().into()),
        JsonValue::Array(a) => {
            match a
                .iter()
                .map(json_value_to_koto_value)
                .collect::<Result<ValueVec, String>>()
            {
                Ok(result) => Value::List(KList::with_data(result)),
                Err(e) => return Err(e),
            }
        }
        JsonValue::Object(o) => {
            let map = KMap::with_capacity(o.len());
            for (key, value) in o.iter() {
                map.insert(key.as_str(), json_value_to_koto_value(value)?);
            }
            Value::Map(map)
        }
    };

    Ok(result)
}

pub fn make_module() -> KMap {
    let result = KMap::with_type("json");

    result.add_fn("from_string", |ctx| match ctx.args() {
        [Value::Str(s)] => match serde_json::from_str(s) {
            Ok(value) => match json_value_to_koto_value(&value) {
                Ok(result) => Ok(result),
                Err(e) => runtime_error!("json.from_string: Error while parsing input: {e}"),
            },
            Err(e) => runtime_error!(
                "json.from_string: Error while parsing input: {}",
                e.to_string()
            ),
        },
        unexpected => type_error_with_slice("a String as argument", unexpected),
    });

    result.add_fn("to_string", |ctx| match ctx.args() {
        [value] => match serde_json::to_string_pretty(&SerializableValue(value)) {
            Ok(result) => Ok(result.into()),
            Err(e) => runtime_error!("json.to_string: {e}"),
        },
        unexpected => type_error_with_slice("a Value as argument", unexpected),
    });

    result
}
