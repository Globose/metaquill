use crate::{arg_parser::PdfData, metadata::PdfStruct};
use std::fs::read_dir;
use std::{fs::File, path::Path};
use std::io::Write;
use serde_json::{json, Value};
use lopdf::{Document, Error as LoError};
use std::error::Error;

pub fn load_pdf(filepath : &str) -> Result<Document, LoError> {
    let document = Document::load(filepath)?;
    return Ok(document);
}

// Returns a list of filepaths of all pdf documents in given directory
// Use rec = true for search in subdirectories
pub fn get_pdf_paths(filepath : &str, rec : bool) -> Option<Vec<String>> {
    let mut pdf_paths : Vec<String> = Vec::new();
    let path = Path::new(filepath);

    if path.is_dir(){
        // If path is a directory
        read_directory(path, &mut pdf_paths, rec);
    } else if path.is_file() {
        // If path is a file
        read_file_path(path, &mut pdf_paths);
    } else {
        println!("The file path has to be a valid pdf file or a directory");
        return None;
    }
    return Some(pdf_paths);
}

// Reads all entries in a directory, adds pdf-files to pdf_paths vector
fn read_directory(path : &Path, pdf_paths : &mut Vec<String>, rec : bool) -> Option<()>{
    // Read all files in directory
    let entries = match read_dir(path) {
        Ok(x) => x,
        Err(e) => {
            println!("Failed to read directory {:?}: {}", path, e);
            return None;
        }
    };

    // Iterate over all directory entries
    for entry_result in entries {
        let Ok(entry) = entry_result else{
            continue;
        };
        
        let entry_path = entry.path();
        if entry_path.is_dir() && rec {
            // If entry is a directory, and recursive search is on
            read_directory(&entry_path, pdf_paths, rec);
        } else if entry_path.is_file(){
            // If entry is a file
            read_file_path(&entry_path, pdf_paths);
        }
    }
    return Some(());
}

// Reads a file path. If it is a .pdf it will be added to pdf_paths
fn read_file_path(path : &Path, pdf_paths : &mut Vec<String>){
    // If path is a file
    let Some(extension) = path.extension() else {
        return;
    };

    // Only care when file extension is .pdf
    if extension != "pdf" {
        return;
    }

    let Some(pdfpath) = path.to_str() else {
        return;
    };
    pdf_paths.push(pdfpath.to_string());
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
