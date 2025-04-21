use std::{fs::read, path::Path};
use lopdf::Document;
use tokio::runtime::Runtime;
use crate::{call::{call, compare_results}, file_manager::{export_json, export_json_metadata, load_pdf}, metadata::{self, fetch_metadata, is_accepted_title, text_to_metadata, PDFStruct}};

/// Reads metadata from pdf
fn read_pdf_metadata (filepath: &str) -> Option<PDFStruct>{
    let document: Document = match load_pdf(filepath) {
        Ok(doc) => doc,
        Err(e) => {return None;}
    };

    // Fetch metadata and assumed title
    let pdf_metadata: PDFStruct = fetch_metadata(&document, filepath);
    Some(pdf_metadata)
}

/// Validates metadata through API
fn validate_metadata(read_metadata : PDFStruct){
    if read_metadata.assumed_title.is_empty() && read_metadata.metadata_title.is_empty(){
        println!("No title found. Skipping Crossref API call.");
        export_json_metadata(&read_metadata);
        return;
    }
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    match runtime.block_on(call(&read_metadata)) {
        Ok(top_score) => {
            let Some(top) = top_score else {
                println!("No API results");
                return;
            };

            // Result cutoff, if no results have a title confidence 70% or higher ignore the results
            if top.title_confidence >= 70.0 {
                export_json(&top, &read_metadata.path); // Export the first metadata entry
            } else {
                println!("Title from API call not close enough");
                export_json_metadata(&read_metadata);
            }
        }
        Err(e) => {
            eprintln!("Error retrieving metadata: {}", e);
            if e.to_string().contains("No metadata found") {
                export_json_metadata(&read_metadata);
            }
        }
    }
}

pub fn read_pdf_dir(path: &Path) -> Option<()>{
    if path.is_dir(){
        let Ok(entries) = std::fs::read_dir(path) else{
            return None;
        };
        for entry in entries {
            let Ok(ent) = entry else{
                continue;
            };
            let ent_path = ent.path();
            read_pdf_dir(&ent_path);
        }
    }
    else{
        let file_path_str = path.to_str().unwrap().to_string();
        read_pdf(&file_path_str);
    }
    return None;
}

/// Removes titles that are wrong, and removes assumed title if it is too similar to metadata title
pub fn evaluate_metadata(pdf : &mut PDFStruct){
    if !is_accepted_title(&pdf.assumed_title){
        // Reject assumed title
        pdf.assumed_title = String::new();
    }
    if !is_accepted_title(&pdf.metadata_title){
        // Reject metadata title
        pdf.metadata_title = String::new();
        return;
    }
    if pdf.assumed_title.is_empty(){
        return;
    }

    let distance = compare_results(&pdf.assumed_title, &pdf.metadata_title);
    if distance > 80.0 {
        pdf.assumed_title = String::new();
    }
}

pub fn read_pdf(filepath: &str){
    println!("{}", filepath);
    let Some(mut pdf) = read_pdf_metadata(filepath) else {
        return;
    };
    println!("Meta = {}", pdf.metadata_title);
    // println!("AssumedTitle = {}", pdf.assumed_title);
    // evaluate_metadata(&mut pdf);
    // println!("2: MetaTitle = {}, AssumedTitle = {}", pdf.metadata_title, pdf.assumed_title);

    // validate_metadata(pdf);
    // println!("Pdf metadata {:?}", pdf);
}
