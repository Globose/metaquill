use lopdf::{Document, Object};
use regex::Regex;
use encoding_rs::WINDOWS_1252;

/// Save the PDF information
pub struct PDFStruct {
    pub path: String,
    pub title: String,
    pub author: Vec<String>,
}

pub fn decode_bytes(bytes: &[u8]) -> String {
    let (cow, _, _) = WINDOWS_1252.decode(bytes); // Decode using Windows-1252
    let s = cow.to_string();
    if s.trim().is_empty() {
        "N/A".to_string()
    } else {
        return s;
    }
}

pub fn fetch_metadata(document : &Document, filepath : String) -> PDFStruct{
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

    return metadata;
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

/// Split the author string into individual names
fn split_authors(input: &str) -> Vec<String> {
    let re = Regex::new(r",|;|\band\b|&").unwrap(); // Split on comma, semicolon, "and" (whole word), or ampersand
    re.split(input)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn print_metadata(metadata: &mut PDFStruct){
    println!("PDF Metadata:");
    println!("Filepath: {}", metadata.path);
    println!("Title: {}", metadata.title);
    println!("Authors: {:?}", metadata.author);
}



