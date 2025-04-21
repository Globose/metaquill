#![allow(dead_code, unused)]

// TODO:
// Add Check for EOL at end of file
// Deny encrypted pdfs

mod decoding;
mod encoding;
mod pdf_object;
mod document;
mod text_parser;

use document::{read_one_pdf, read_pdf_in_dir};
use encoding::PDFDOC_MAP;

fn main() {
    read_all();
    // read_one();
}

fn read_all(){
    let fpath = r"C:\data\projekt\pdf-to-metadata\pdfs";
    if let Ok(cnt) = read_pdf_in_dir(fpath){
        println!("Pdfs read: {}", cnt.0);
        println!("Pdfs accepted title: {}", cnt.1);
    };
}

fn read_one(){
    // let filepath = r"C:\data\projekt\pdf-to-metadata\pdf_1k\elsevier\els10.pdf";
    let filepath = r"C:\data\projekt\pdf-to-metadata\pdfs\s9.pdf";
    
    match read_one_pdf(filepath) {
        Ok(mut x) =>{
            if let Some(text_objects) = x.get_text_from_page(0){
                println!("Textobjects {}", text_objects.len());
                for text_obj in text_objects{
                    println!("---");
                    println!("Font size: {}", text_obj.scaled_font_size);
                    println!("Y: {}", text_obj.pos_y);
                    println!("Text: {}", text_obj.chars);
                }
            };
            if let Ok(x) = x.get_info("Creator"){
                println!("Tilte {}", x);
            }
            match x.get_info("Title"){
                Ok(x) => println!("Info-record: {}", x),
                Err(e) => println!("Ierr {:?}", e),
            }
        }
        Err(e) =>{
            println!("Error {:?}", e);
        }
    }
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

