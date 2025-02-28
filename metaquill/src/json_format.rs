use std::collections::HashMap;
use crate::metadata::PDFStruct;
use std::fs::File;
use std::io::{BufWriter, Write};
use serde_json::{json, Value};

fn make_json(key: &str, value: Value) -> Value {
    let mut json = HashMap::new();
    json.insert(key.to_string(),value);
    serde_json::to_value(json).unwrap()
}

pub fn export_json(pdf_metadata : &PDFStruct){
    // Prepare the data for JSON formatting
    let key_name = "Metadata";  // This is a test key
    let input_value = [pdf_metadata.title.clone(), pdf_metadata.author.join(" ")];  // Combine title and authors

    // Print the JSON output
    let json_value = make_json(key_name, json!(input_value));
    println!("{}", serde_json::to_string_pretty(&json_value).unwrap());

   if let Err(e) = create_file(json_value) {
       eprintln!("Error creating file: {}", e);
   }
}

pub fn create_file(value: Value) -> std::io::Result<()> {
    let file = File::create("output.json")?;
    let mut writer = BufWriter::new(file);

    let json_string = serde_json::to_string_pretty(&value)?; // Serialize JSON to string
    writer.write_all(json_string.as_bytes())?;
    writer.flush()?;

    Ok(())
}
