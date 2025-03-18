use load::load_pdf;
use metadata::fetch_metadata;
use json_format::export_json;
use metadata::PDFStruct;
use std::env;
use lopdf::Document;
use call::call;
use tokio::runtime::Runtime; // ✅ Import Tokio runtime

mod json_format;
mod metadata;
mod load;
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
    let pdf_metadata: PDFStruct = fetch_metadata(document, filepath);
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    match runtime.block_on(call(&pdf_metadata)) {
        Ok(_) => println!("Metadata retrieved successfully!"),
        Err(e) => eprintln!("Error retrieving metadata: {}", e),
    }
    // export_json(&pdf_metadata);
}

