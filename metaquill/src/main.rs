use load::load_pdf;
use metadata::fetch_metadata;
use text_parser::text_to_metadata;
use json_format::export_json;
use metadata::PDFStruct;
use std::env;
use lopdf::Document;
use call::call;
use tokio::runtime::Runtime; // Import Tokio runtime
use std::fs;
use std::path::Path;

mod json_format;
mod metadata;
mod load;

mod text_parser;

mod call;


fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Failed to read PDF: No PDF file provided");
        return;
    }

    // Load the PDF file
    let filepath: String = args[1].clone();
    let path = Path::new(&args[1]);

    if path.is_dir() {
        println!("It's a directory.");
    } else {
        println!("It's a file.");
        data_extract(filepath);
    }

    

}



fn data_extract (filepath: String) {
    let document: Document = match load_pdf(&filepath) {
        Ok(doc) => doc,
        Err(e) => {
            println!("Failed to load PDF: {}", e);
            return;
        }
    };

    // Fetch metadata, create JSON

    let mut pdf_metadata: PDFStruct = fetch_metadata(&document, filepath);
  
    // Text to metadata
    let temp_title = text_to_metadata(&document);
    if pdf_metadata.title.is_empty() || pdf_metadata.title == "N/A" {
        pdf_metadata.title = temp_title;
    }


    if !pdf_metadata.title.trim().is_empty() && pdf_metadata.title.trim() != "N/A" {
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        match runtime.block_on(call(&pdf_metadata)) {
            Ok(metadata_list) => {
                if let Some(first_metadata) = metadata_list.get(0) {
                    export_json(first_metadata); // Export the first metadata entry
                } else {
                    println!("No valid metadata found.");
                }
            }
            Err(e) => {
                eprintln!("Error retrieving metadata: {}", e);
                if e.to_string().contains("Try OCR extraction") {
                    println!("🔍 Attempting OCR-based title extraction...");
                }
            }
        }
    } else {
        println!("No title found. Skipping Crossref API call.");
    }

}