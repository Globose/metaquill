use load::load_pdf;
use metadata::fetch_metadata;
use json_format::export_json;
use metadata::PDFStruct;
use std::env;
use lopdf::Document;

mod json_format;
mod metadata;
mod load;

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
    let pdf_metadata: PDFStruct = fetch_metadata(document, filepath);
    export_json(&pdf_metadata);
}

