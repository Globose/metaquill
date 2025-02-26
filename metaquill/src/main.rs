use load::load_pdf;
use std::env;
use lopdf::{Document, Object};

// import to json
use serde_json:: json;
use json_format1::json_format;

// import to decoding
use encoding_rs::WINDOWS_1252;
use regex::Regex;

use std::io::{self, Read};
use std::collections::BTreeMap;
use lopdf::content::{Content, Operation};






mod json_format1;
mod load;

/// Save the PDF information
struct PDFStruct {
    path: String,
    title: String,
    author: Vec<String>,
}

// / Decode unknown characters
fn decode_bytes(bytes: &[u8]) -> String {
    let (cow, _, _) = WINDOWS_1252.decode(bytes); // Decode using Windows-1252
    let s = cow.to_string();
    if s.trim().is_empty() {
        "N/A".to_string()
    } else {
        return s;
    }
}


// fn decode_bytes(bytes: &[u8]) -> String {
//     let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
//     let s = cow.to_string();
//     if s.trim().is_empty() {
//         "N/A".to_string()
//     } else {
//         s
//     }
// }

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
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Failed to read PDF: No PDF file provided");
        return;
    }

    // Load the PDF file
    let filepath = args[1].clone();
    let document = match load_pdf(&filepath) {
        Ok(doc) => {
            println!("Document successfully loaded");
            doc
        }
        Err(e) => {
            println!("Failed to load PDF: {}", e);
            return;
        }
    };

    // Print the number of pages in the PDF
    println!("PDF Page count: {}", document.get_pages().len());

    // Create a struct for metadata
    let mut metadata = PDFStruct {
        path: filepath.clone(),
        title: String::new(),
        author: Vec::new(),
    };

    // Extract metadata from the file header
    collect_title_and_author(&document, &mut metadata);

    // Print the PDF metadata
    print_metadata(&mut metadata);

    // Prepare the data for JSON formatting
    let key_name = "Metadata";  // This is a test key
    let input_value = [metadata.title.clone(), metadata.author.join(" ")];  // Combine title and authors

    let (t_title, t_authors)  = text_to_meta(&document);
    println!("Title_t: {}", t_title);
    println!("Authors_t: {:?}", t_authors);
    // Print the JSON output
    let x = json_format(key_name, json!(input_value));
    println!("{}", serde_json::to_string_pretty(&x).unwrap());

}


fn text_to_meta(document: &Document) -> (String, Vec<String>) {
    // A vector to hold tuples of (extracted text, current font size)
    let mut text_items: Vec<(String, f32)> = Vec::new();

    // Process only the first page (page 1).
    if let Some(&page_id) = document.get_pages().get(&1) {
        if let Ok(page_content) = document.get_page_content(page_id) {
            if let Ok(content) = Content::decode(&page_content) {
                let mut current_font_size: f32 = 0.0;
                for operation in content.operations {
                    match operation.operator.as_ref() {
                        // "Tf" sets the font and its size. Its operands are usually [font_ref, font_size]
                        "Tf" => {
                            if let Some(size_obj) = operation.operands.get(1) {
                                if let Ok(size) = size_obj.as_i64() {
                                    current_font_size = size as f32;
                                }
                            }
                        }
                        // "Tj" draws a string
                        "Tj" => {
                            if let Some(text_obj) = operation.operands.get(0) {
                                if let Ok(text) = text_obj.as_str() {
                                    let decoded = decode_bytes(text);
                                    text_items.push((decoded, current_font_size));
                                }
                            }
                        }
                        // "TJ" draws an array of strings (and spacing adjustments)
                        "TJ" => {
                            if let Some(array_obj) = operation.operands.get(0) {
                                if let Ok(array) = array_obj.as_array() {
                                    let mut combined = String::new();
                                    for item in array {
                                        if let Ok(s) = item.as_str() {
                                            combined.push_str(&String::from_utf8_lossy(s));
                                        }
                                    }
                                    let decoded = decode_bytes(combined.as_bytes());
                                    text_items.push((decoded, current_font_size));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    } else {
        eprintln!("Page 1 not found in the document.");
    }

    // Debug: Uncomment the next lines to print each text segment with its font size.
    for (t, s) in &text_items {
        println!("Font size: {:.2} | Text: {}", s, t);
    }

    // Heuristic 1: Pick the text with the maximum font size as the title candidate.
    let mut title_candidate = String::new();
    let mut max_font_size = 0.0;
    for (text, size) in &text_items {
        // Skip overly short strings.
        if text.trim().len() > 5 && *size > max_font_size {
            max_font_size = *size;
            title_candidate = text.clone();
        }
    }

    // Heuristic 2: After the title candidate, pick the next text block that is not too long (e.g., <150 chars)
    // as the authors’ candidate.
    let mut author_candidate = String::new();
    let mut found_title = false;
    for (text, _) in &text_items {
        if text == &title_candidate {
            found_title = true;
            continue;
        }
        if found_title {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() < 150 {
                author_candidate = trimmed.to_string();
                break;
            }
        }
    }

    // Use split_authors to get a list of individual author names.
    let authors = if !author_candidate.is_empty() {
        split_authors(&author_candidate)
    } else {
        vec!["N/A".to_string()]
    };

    let title = if title_candidate.is_empty() {
        "N/A".to_string()
    } else {
        title_candidate
    };

    (title, authors)
}

