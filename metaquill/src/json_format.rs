use std::collections::HashMap;
use serde_json:: Value;
use crate::metadata::PDFStruct;
use serde_json:: json;


fn json_format(key: &str, value: Value) -> Value {
    let mut json = HashMap::new();
    json.insert(key.to_string(),value);
    serde_json::to_value(json).unwrap()
}

pub fn export_json(pdf_metadata : &PDFStruct){
    // Prepare the data for JSON formatting
    let key_name = "Metadata";  // This is a test key
    let input_value = [pdf_metadata.title.clone(), pdf_metadata.author.join(" ")];  // Combine title and authors

    // Print the JSON output
    let json_value = json_format(key_name, json!(input_value));
    println!("{}", serde_json::to_string_pretty(&json_value).unwrap());
}
