#![allow(dead_code, unused)]
use std::{env, slice::RSplit};
use std::path::Path;
use call::Metadata;
use document::{read_pdf_dir, PdfResult};
use file_manager::{close_file, create_file, export_csv};
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

    let mut result = PdfResult{pdfs : Vec::new(), read : 0, fails : 0};
    read_pdf_dir(Path::new(&args[1]), &mut result);
    println!("Tried to read {} files, {} failed", result.read, result.fails);
    export_csv(&mut result.pdfs);
}
