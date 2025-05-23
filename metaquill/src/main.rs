// #![allow(dead_code, unused)]
use std::env;
use arg_parser::parse_args;
use document::read_pdf_dir;
use file_manager::{export_csv, export_json, get_pdf_paths};
mod metadata;
mod call;
mod file_manager;
mod document;
mod arg_parser;

fn main() {
    // Parse Arguments
    let args: Vec<String> = env::args().collect();
    let Some(mut pdf_data) = parse_args(&args) else {
        return;
    };

    // Get all pdf:s in path
    let Some(pdf_paths) = get_pdf_paths(&pdf_data.path, pdf_data.recursive) else {
        return;
    };

    println!("Attempting to read {} pdf documents", pdf_paths.len());
    read_pdf_dir(&pdf_paths, &mut pdf_data);
    
    println!("---");
    println!("Tried to read {} files, {} failed, {} timeouts, {} api-hits", pdf_data.read, pdf_data.fails, pdf_data.timeouts, pdf_data.api_hits);
    
    // Output result to a json file
    if let Err(err) = export_json(&mut pdf_data){
        println!("{}", err);
    };
    
    // Output result to a csv file
    if let Err(err) = export_csv(&mut pdf_data.pdfs){
        println!("{}", err);
    };
}
