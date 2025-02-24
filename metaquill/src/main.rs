use load::load_pdf;
use std::env;
use lopdf::{Document, Object};
use std::collections::HashMap;
use serde_json:: {json,Value};

mod load;

/// Save the PDF information
struct PDFStruct {
    path: String,
    title: String,
    author: String,
}

/// Collects the Title and Author from the PDF's trailer "Info" dictionary.
fn collect_title_and_author(document: &Document, metadata: &mut PDFStruct) {
    // Get the "Info" entry from the trailer, if available.
    if let Ok(Some(Object::Dictionary(dict))) = document.trailer.get(b"Info").map(|obj| match obj {
        Object::Reference(id) => document.get_dictionary(*id).ok().map(|d| Object::Dictionary(d.clone())),
        Object::Dictionary(d) => Some(Object::Dictionary(d.clone())),
        _ => None,
    }) {
        metadata.title = dict
        .get(b"Title")
        .and_then(|v| v.as_str())
        .map(|s| {
            // maybe look for way to fix some symbols (utf8)
            let s = String::from_utf8_lossy(s).to_string();
            if s.trim().is_empty() { "N/A".to_string() } else { s }
        })
        .unwrap_or_else(|_| "N/A".to_string()); // Correct closure for Option
    
    metadata.author = dict
        .get(b"Author")
        .and_then(|v| v.as_str())
        .map(|s| {
            // maybe look for way to fix some symbols (utf8)
            let s = String::from_utf8_lossy(s).to_string();
            if s.trim().is_empty() { "N/A".to_string() } else { s }
        })
        .unwrap_or_else(|_| "N/A".to_string()); // Correct closure for Option
    
    } else {
        // Set default values if no Info dictionary is found
        metadata.title = "N/A".to_string();
        metadata.author = "N/A".to_string();
    }

}

fn print_metadata(metadata: &mut PDFStruct){
    println!("PDF Metadata:");
    println!("Filepath: {}", metadata.path);
    println!("Title: {}", metadata.title);
    println!("Author: {}", metadata.author);
}

fn main() {
    // Collect arguments
    let args : Vec<String> = env::args().collect();
    let key_name = "ListNumbers"; // This is a test for json_format file. this is a key
    let input_value = [3,2,1]; // This is a value

    """here we print the json as a string"""
    let x = json_format(key_name, json!(input_value));
    println!("{}",serde_json::to_string_pretty(&x).unwrap());
    
    // temporary, more args will be accepted later on
    if args.len() != 2{
        println!("Failed to read PDF: No pdf given");
        return;
    }
    
    // Load PDF file
    let filepath : String = args[1].clone();
    let document = match load_pdf(&filepath){
        Ok(doc) => {
            println!("Document successfully loaded");
            doc
        }
        Err(e) => {
            println!("Failed to load PDF: {e}");
            return;
        }
    };

    // Print page count
    println!("PDF Page count: {}", document.get_pages().len());

    // Create a struct
    let mut metadata = PDFStruct {
        path: filepath.clone(),
        title: String::new(),
        author: String::new(),
    };
    // Read Filehead to get metadata
    collect_title_and_author(&document, &mut metadata);
    //Print the Informatioon of the pdf
    print_metadata(&mut metadata)
}


