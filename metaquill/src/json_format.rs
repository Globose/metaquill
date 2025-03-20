use crate::call::Metadata;
use std::fs::File;
use std::io::{BufWriter, Write};
use serde_json::{json, Value};


pub fn export_json(extracted_meta: &Metadata) {
    // Prepare structured metadata for JSON output
    let json_value = json!({
        "Title": extracted_meta.title,
        "Authors": extracted_meta.authors,
        "DOI": extracted_meta.doi,
        "Score": extracted_meta.score,
        "Publisher": extracted_meta.publisher,
        "Journal": extracted_meta.journal,
        "Year": extracted_meta.year,
        "Volume": extracted_meta.volume,
        "Issue": extracted_meta.issue,
        "Pages": extracted_meta.pages,
        "ISSN": extracted_meta.issn,
        "URL": extracted_meta.url,
    });

    // Print JSON to console in a readable format
    println!("{}", serde_json::to_string_pretty(&json_value).unwrap());

    // Save JSON to a file
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
