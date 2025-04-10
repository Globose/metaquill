#![allow(dead_code, unused)]
use std::env;
use std::path::Path;
use document::read_pdf_dir;
use file_manager::{create_file, close_file};
mod metadata;
mod call;
mod file_manager;
mod document;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Failed to read PDF: No PDF file provided");
        return;
    }

    // Create ouputfile
    create_file();
    // Load the PDF file
    read_pdf_dir(Path::new(&args[1]));
    // End json output
    close_file();
}
