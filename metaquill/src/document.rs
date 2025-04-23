use std::path::Path;
use lopdf::Document;
use tag_pdf_to_text::load_pdf_doc;
use tokio::runtime::Runtime;
use crate::file_manager::{export_json, export_json_metadata, load_pdf};
use crate::metadata::{extract_metadata, fetch_metadata, get_probable_title, is_accepted_title, PDFStruct};
use crate::call::{call, compare_results};

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
        let Some(extension) = path.extension() else {
            return None;
        };
        if extension != "pdf" {
            return None;
        }
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
    println!("---");
    println!("{}", filepath);

    // LOPDF:
    // match read_pdf_metadata(filepath) {
    //     Some(mut lo_pdf) => {
    //         println!("AssumedTitle = {}", lo_pdf.metadata_title);
    //         println!("AssumedTitle = {}", lo_pdf.assumed_title);
    //         evaluate_metadata(&mut lo_pdf);
    //         validate_metadata(lo_pdf);
    //     },
    //     None => {
    //         println!("Lo: fail");
    //     }
    // };
    
    // TAG-PDF:
    match load_pdf_doc(filepath) {
        Ok(mut pdf) => {
            let mut pdf_meta = extract_metadata(&mut pdf, filepath);
            println!("MetaTitle = {}", pdf_meta.metadata_title);
            println!("AssumedTitle = {}", pdf_meta.assumed_title);
            // evaluate_metadata(&mut pdf_meta);
            // validate_metadata(pdf_meta);
        }
        Err(e) =>{
            println!("Err: {:?}", e);
        }
    };
}
