use std::collections::HashMap;
use serde_json:: Value;

pub fn json_format(key: &str, value: Value) -> Value {
    let mut json = HashMap::new();
    json.insert(key.to_string(),value);
    serde_json::to_value(json).unwrap()
}

