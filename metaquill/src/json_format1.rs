use std::collections::HashMap;
use serde_json:: {json,Value};

pub fn json_format(key: &str, value: Value) -> Value {
    let mut JSON = HashMap::new();
    JSON.insert(key.to_string(),value);
    serde_json::to_value(JSON).unwrap()
}

