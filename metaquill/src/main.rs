use load::load_pdf;
use metadata::fetch_metadata;
use text_parser::text_to_metadata;
use json_format::{export_json, export_json_metadata};
use metadata::PDFStruct;
use std::env;
use lopdf::Document;
use call::call;
use tokio::runtime::Runtime; // Import Tokio runtime

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
    let document: Document = match load_pdf(&filepath) {
        Ok(doc) => doc,
        Err(e) => {
            println!("Failed to load PDF: {}", e);
            return;
        }
    };

    // Fetch metadata, create JSON

    let mut pdf_metadata: PDFStruct = fetch_metadata(&document, filepath.clone());

    // Text to metadata
    let temp_title = text_to_metadata(&document);
    if pdf_metadata.title.is_empty() || pdf_metadata.title == "N/A" {
        pdf_metadata.title = temp_title;
    }


    if !pdf_metadata.title.trim().is_empty() && pdf_metadata.title.trim() != "N/A" {
        // Create runtime to let the program wait for a response
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        match runtime.block_on(call(&pdf_metadata)) {
            Ok(metadata_list) => {
                if let Some(first_metadata) = metadata_list.get(0) {
                    // Result cutoff, if no results have a title confidence 70% or higher ignore the results
                    if first_metadata.title_confidence >= 70.0 {
                    export_json(first_metadata, filepath); // Export the first metadata entry
                    } else {
                        println!("Title from API call not close enough");
                    }

                } else {
                    export_json_metadata(&pdf_metadata);
                }
            }
            Err(e) => {
                eprintln!("Error retrieving metadata: {}", e);
                if e.to_string().contains("No metadata found") {
                    export_json_metadata(&pdf_metadata);
                }
            }
        }
    } else {
        println!("No title found. Skipping Crossref API call.");
        export_json_metadata(&pdf_metadata);
    }

}

