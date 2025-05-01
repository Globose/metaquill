use crate::metadata::PdfStruct;
use crate::PdfData;
use std::fs::File;
use std::io::Write;
use serde_json::{json, Value};
use lopdf::{Document, Error as LoError};
use std::error::Error;

pub fn load_pdf(filepath : &str) -> Result<Document, LoError> {
    // println!("Loading PDF {filepath}");
    let document = Document::load(filepath)?;
    return Ok(document);
}

/// Export a list of read PDFs to a json file
pub fn export_json(pdf_data : &mut PdfData) -> Result<(), Box <dyn Error>> {
    // Prepare structured metadata for JSON output
    let mut json_data : Vec<Value> = Vec::new();
    let pdfs = &mut pdf_data.pdfs;

    for pdf in pdfs{
        if let Some(api_meta) = &pdf.api_metadata {
            // If api metadata exist
            let json_value = json!({
                "file_name": pdf.filename,
                "title": api_meta.title,
                "title_confidence": api_meta.title_confidence,
                "authors": api_meta.authors,
                "doi": api_meta.doi,
                "api_score": api_meta.score,
                "publisher": api_meta.publisher,
                "journal": api_meta.journal,
                "year": api_meta.year,
                "volume": api_meta.volume,
                "issue": api_meta.issue,
                "pages": api_meta.pages,
                "issn": api_meta.issn,
                "url": api_meta.url,
            });
            json_data.push(json_value);
        } else {
            // If no api metadata is found
            // Choose a title to use
            let mut json_title = Value::Null;
            if !pdf.metadata_title.is_empty(){
                json_title = Value::String(pdf.metadata_title.clone());
            } else if !pdf.assumed_title.is_empty() {
                json_title = Value::String(pdf.assumed_title.clone());
            }

            // Choose if to include authors. If more than one author, it's a vector
            let mut json_authors = Value::Null;
            if pdf.author.len() > 0 {
                let mut auth_vector : Vec<Value> = Vec::new();
                for auth in &pdf.author{
                    auth_vector.push(Value::String(auth.clone()));
                }

                if auth_vector.len() == 1{
                    json_authors = auth_vector[0].clone();
                }
                else {
                    json_authors = Value::Array(auth_vector);
                }
            }

            let json_value = json!({
                "file_name": pdf.filename,
                "title": json_title,
                "title_confidence": null,
                "authors": json_authors,
                "doi": null,
                "api_score": null,
                "publisher": null,
                "journal": null,
                "year": null,
                "volume": null,
                "issue": null,
                "pages": null,
                "issn": null,
                "url": null,
            });
            json_data.push(json_value);
        }
    }

    // Save to json file
    let json_array = serde_json::Value::Array(json_data);
    let Ok(json_str) = serde_json::to_string_pretty(&json_array) else {
        return Err("Failed to create json".into());
    };

    // Create a json file
    let mut file = match File::create(pdf_data.output_filepath.as_str()) {
        Ok(x) => x,
        Err(err) => {
            let err_msg = format!("Failed to create json file: {}", err);
            return Err(err_msg.into());
        }
    };
    
    // Write to json file
    if let Err(err) = file.write_all(json_str.as_bytes()){
        let err_msg = format!("Failed to write to json file: {}", err);
        return Err(err_msg.into());
    };
    Ok(())
}

/// Get last part (filename) of filepath
pub fn split_name(filepath: &str) -> Option<String>{
    // Split by slash and take the last part
    let normalized = filepath.replace('\\', "/");
    
    normalized
        .split('/')
        .last()
        .map(|s| s.to_string())
}

// Exports the results to a csv file
pub fn export_csv(pdfs : &mut Vec<PdfStruct>) -> Result<(), Box <dyn Error>>{
    let Ok(mut csv_file) = File::create("meta.csv") else {
        // println!("Failed to create csv");
        return Err("Failed to create a csv file".into());
    };
    
    // Natural sort based on the file name
    pdfs.sort_by(|m1,m2| natord::compare(&m1.filename, &m2.filename));

    // Write CSV header
    if let Err(err) = writeln!(csv_file, "file,api_title,info_title,assumed_title,year"){
        let err_msg = format!("Failed to write to csv-file: {}", err);
        return Err(err_msg.into());
    };
    
    // Write one entry for each PDF
    for pdf in pdfs{
        let mut line = String::new();

        // Filename
        line.push_str(&pdf.filename.as_str());
        line.push_str(",");
        
        // Title
        line.push('"');
        if let Some(api_meta) = &pdf.api_metadata{
            line.push_str(api_meta.title.as_str());
        };
        line.push('"');
        line.push(',');
        
        // Info title
        line.push('"');
        line.push_str(&pdf.metadata_title);
        line.push('"');
        line.push(',');
        
        // Assumed title
        line.push('"');
        line.push_str(&pdf.assumed_title);
        line.push('"');
        line.push(',');
        
        // Year
        if let Some(api_meta) = &pdf.api_metadata{
            line.push_str(api_meta.year.to_string().as_str());
        };

        if let Err(err) = writeln!(csv_file, "{line}"){
            let err_msg = format!("Failed to write to csv-file: {}", err);
            return Err(err_msg.into());
        };
    }
    Ok(())
}
