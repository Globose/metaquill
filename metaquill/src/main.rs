// #![allow(dead_code, unused)]
use std::env;
use std::path::Path;
use document::read_pdf_dir;
use file_manager::{export_csv, export_json};
use metadata::PdfStruct;
mod metadata;
mod call;
mod file_manager;
mod document;

#[derive(Debug)]
pub struct PdfData {
    pub pdfs : Vec<PdfStruct>,
    pub read : u32,
    pub fails : u32,
    pub api_hits : u32,
    pub output_filepath : String,
    pub reader : u8, // 1 = lopdf, 0 else
    pub print_info : bool,
    pub print_api_url : bool,
    pub make_api_call : bool,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Failed to read PDF: No PDF file provided");
        return;
    }

    // PDF reading settings
    let mut pdf_data = PdfData{pdfs : Vec::new(), read : 0, fails : 0, reader : 0, 
        output_filepath : "output.json".to_string(), print_info: true, make_api_call : false,
        print_api_url: false, api_hits : 0};

    // pdf_data.reader = 1;
    read_pdf_dir(Path::new(&args[1]), &mut pdf_data);
    println!("---");
    println!("Tried to read {} files, {} failed", pdf_data.read, pdf_data.fails);
    
    // Output result to a csv file
    if let Err(err) = export_csv(&mut pdf_data.pdfs){
        println!("Error: {}", err);
    };
    
    // Output result to a json file
    if let Err(err) = export_json(&mut pdf_data){
        println!("{}", err);
    };
}
