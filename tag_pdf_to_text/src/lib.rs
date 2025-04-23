#![allow(dead_code, unused)]

use document::{read_one_pdf, Document, PdfError};

mod decoding;
pub mod document;
mod encoding;
mod pdf_object;
mod text_parser;

pub fn load_pdf_doc(filepath : &str) -> Result<Document, PdfError> {
    read_one_pdf(filepath)
}

fn print_raw(doc_u8 : &Vec<u8>, ix : usize, size : usize){
    println!("\nRAW");
    for i in ix..ix+size{
        if i >= doc_u8.len(){
            return;
        }
        if (32..127).contains(&doc_u8[i]){ 
            print!("{}",doc_u8[i] as char);
        } 
        else if matches!(doc_u8[i], 10 | 13){
            println!("");
        }
        else{
            print!("[{}]", doc_u8[i]);
        }
    }
}
