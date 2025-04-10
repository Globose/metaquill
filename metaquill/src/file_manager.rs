use crate::call::Metadata;
use std::fs::File;
use std::io::{BufWriter, Write};
use serde_json::{json, Value};
use lopdf::{Document, Error};
use crate::metadata::PDFStruct;

pub fn load_pdf(filepath : &str) -> Result<Document, Error> {
    // println!("Loading PDF {filepath}");
    let document = Document::load(filepath)?;
    return Ok(document);
}

pub fn export_json(extracted_meta: &Metadata, filepath: &str) {
    // Prepare structured metadata for JSON output
    let json_value = json!({
        "Title": extracted_meta.title,
        "Authors": extracted_meta.authors,
        "DOI": extracted_meta.doi,
        "API Score": extracted_meta.score,
        "Publisher": extracted_meta.publisher,
        "Journal": extracted_meta.journal,
        "Year": extracted_meta.year,
        "Volume": extracted_meta.volume,
        "Issue": extracted_meta.issue,
        "Pages": extracted_meta.pages,
        "ISSN": extracted_meta.issn,
        "URL": extracted_meta.url,
        "Title Confidence": extracted_meta.title_confidence.to_string() + "%",
        "PDF Name": split_name(filepath.to_string()),
    });

    // Print JSON to console in a readable format
    println!("{}", serde_json::to_string_pretty(&json_value).unwrap());

    // Save JSON to a file
    if let Err(e) = create_file(json_value) {
        eprintln!("Error creating file: {}", e);
    }
}

pub fn split_name(filepath: String) -> Option<String>{
    // Split by slash and take the last part
    let normalized = filepath.replace('\\', "/");
    
    normalized
        .split('/')
        .last()
        .map(|s| s.to_string())
}

pub fn export_json_metadata(pdf_metadata : &PDFStruct){
    // Prepare the data for JSON formatting

    let json_value = json!({
        "Title": pdf_metadata.metadata_title.clone(),
        "Authors": pdf_metadata.author.clone(),
        "DOI": "N/A",
        "API Score": "N/A",
        "Publisher": "N/A",
        "Journal": "N/A",
        "Year": "N/A",
        "Volume": "N/A",
        "Issue": "N/A",
        "Pages": "N/A",
        "ISSN": "N/A",
        "URL": "N/A",
        "Title Confidence": "N/A",
        "PDF Name": split_name(pdf_metadata.path.clone()),
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
