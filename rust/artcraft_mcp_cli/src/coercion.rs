//! Schema-guided argument coercion (Phase 2): LLMs frequently pass numbers,
//! booleans, and enums as strings. Coerce stringified primitives into the
//! types declared by each tool's input schema before dispatch.

use serde_json::{Map, Value};

/// Coerce values in `object` toward the types declared in `schema`.
/// Only touches properties whose schema type is a primitive (integer, number,
/// boolean); strings are left as-is; nested objects recurse.
pub fn coerce_object(object: Map<String, Value>, schema: &Value) -> Map<String, Value> {
    let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) else {
        return object;
    };

    let mut out = Map::new();
    for (key, value) in object.into_iter() {
        let prop_schema = properties.get(&key).cloned().unwrap_or(Value::Null);
        out.insert(key, coerce_value(value, &prop_schema));
    }
    out
}

fn coerce_value(value: Value, schema: &Value) -> Value {
    // Resolve $ref-free simple cases; union types pick the first non-string match.
    let ty = effective_type(schema);

    match (value, ty) {
        (Value::String(s), Some("integer")) => {
            if let Ok(n) = s.trim().parse::<i64>() {
                Value::from(n)
            } else if let Ok(f) = s.trim().parse::<f64>() {
                // tolerate "5.0" for integer fields
                Value::from(f as i64)
            } else {
                Value::String(s)
            }
        }
        (Value::String(s), Some("number")) => match s.trim().parse::<f64>() {
            Ok(f) => Value::from(f),
            Err(_) => Value::String(s),
        },
        (Value::String(s), Some("boolean")) => match s.trim().to_lowercase().as_str() {
            "true" | "1" | "yes" => Value::Bool(true),
            "false" | "0" | "no" => Value::Bool(false),
            _ => Value::String(s),
        },
        (Value::Object(map), Some("object")) => Value::Object(coerce_object(map, schema)),
        (Value::Array(items), Some("array")) => {
            let item_schema = schema.get("items").cloned().unwrap_or(Value::Null);
            Value::Array(
                items
                    .into_iter()
                    .map(|v| coerce_value(v, &item_schema))
                    .collect(),
            )
        }
        // A string passed for an array of one item: wrap if single value.
        (v, _) => v,
    }
}

fn effective_type(schema: &Value) -> Option<&str> {
    if let Some(t) = schema.get("type").and_then(|t| t.as_str()) {
        return Some(t);
    }
    // anyOf/oneOf unions: prefer first non-string primitive.
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = schema.get(key).and_then(|v| v.as_array()) {
            let mut first_string = None;
            for variant in variants {
                if let Some(t) = variant.get("type").and_then(|t| t.as_str()) {
                    if t != "string" {
                        return Some(t);
                    }
                    first_string = first_string.or(Some(t));
                }
            }
            if first_string.is_some() {
                return first_string;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerces_stringified_numbers_and_bools() {
        let schema = json!({
            "type": "object",
            "properties": {
                "width": {"type": "integer"},
                "strength": {"type": "number"},
                "fast": {"type": "boolean"},
                "name": {"type": "string"}
            }
        });
        let mut obj = Map::new();
        obj.insert("width".into(), Value::String("1024".into()));
        obj.insert("strength".into(), Value::String("0.75".into()));
        obj.insert("fast".into(), Value::String("true".into()));
        obj.insert("name".into(), Value::String("test".into()));

        let out = coerce_object(obj, &schema);
        assert_eq!(out["width"], json!(1024));
        assert_eq!(out["strength"], json!(0.75));
        assert_eq!(out["fast"], json!(true));
        assert_eq!(out["name"], json!("test"));
    }
}
