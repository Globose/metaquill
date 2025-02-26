use load::load_pdf;
use std::env;
use lopdf::{Document, Object};

use serde_json:: json;
use json_format1::json_format;

use encoding_rs::WINDOWS_1252;
use regex::Regex;

mod json_format1;
mod load;

/// Save the PDF information
struct PDFStruct {
    path: String,
    title: String,
    author: Vec<String>,
}

/// Decode unknown characters
fn decode_bytes(bytes: &[u8]) -> String {
    let (cow, _, _) = WINDOWS_1252.decode(bytes); // Decode using Windows-1252
    let s = cow.to_string();
    if s.trim().is_empty() {
        "N/A".to_string()
    } else {
        s
    }
}

/// Split the author string into individual names
fn split_authors(input: &str) -> Vec<String> {
    let re = Regex::new(r",|;|\band\b|&").unwrap(); // Split on comma, semicolon, "and" (whole word), or ampersand
    re.split(input)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Collects the Title and Author from the PDF's trailer "Info" dictionary.
fn collect_title_and_author(document: &Document, metadata: &mut PDFStruct) {
    // Get the "Info" entry from the trailer, if available.
    if let Ok(Some(Object::Dictionary(dict))) = document.trailer.get(b"Info").map(|obj| match obj {
        Object::Reference(id) => document.get_dictionary(*id).ok().map(|d| Object::Dictionary(d.clone())),
        Object::Dictionary(d) => Some(Object::Dictionary(d.clone())),
        _ => None,
    }) {
        // Extract and decode Title
        metadata.title = dict
            .get(b"Title")
            .and_then(|v| v.as_str())
            .map(decode_bytes)
            .unwrap_or_else(|_| "N/A".to_string());

        // Extract and decode Author
        metadata.author = dict
            .get(b"Author")
            .and_then(|v| v.as_str())
            .map(|s: &[u8]| split_authors(&decode_bytes(s)))
            .unwrap_or_else(|_| vec!["N/A".to_string()]);

    } else {
        // Set default values if no Info dictionary is found
        metadata.title = "N/A".to_string();
        metadata.author = vec!["N/A".to_string()];
    }
}

fn print_metadata(metadata: &mut PDFStruct){
    println!("PDF Metadata:");
    println!("Filepath: {}", metadata.path);
    println!("Title: {}", metadata.title);
    println!("Authors: {:?}", metadata.author);
}

fn main() {
    // Collect arguments
    let args : Vec<String> = env::args().collect();


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
        author: Vec::new(),
    };
    // Read Filehead to get metadata
    collect_title_and_author(&document, &mut metadata);
    //Print the Informatioon of the pdf
    print_metadata(&mut metadata);

    let key_name = "Title"; // This is a test for json_format file. this is a key
    let input_value = metadata.title; // This is a value

    //here we print the json as a string
    let x = json_format(key_name, json!(input_value));
    println!("{}",serde_json::to_string_pretty(&x).unwrap());
}


