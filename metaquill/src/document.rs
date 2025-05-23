use std::error::Error;
use lopdf::Document;
use tag_pdf_to_text::load_pdf_doc;
use tokio::runtime::Runtime;
use crate::arg_parser::{Verbose, PdfData};
use crate::file_manager::load_pdf;
use crate::metadata::{extract_metadata, fetch_metadata, PdfStruct};
use crate::call::{call, PdfMetadata};

/// Reads metadata from pdf
fn read_pdf_metadata (filepath: &str) -> Option<PdfStruct>{
    let document: Document = match load_pdf(filepath) {
        Ok(doc) => doc,
        Err(_) => {return None;}
    };

    // Fetch metadata and assumed title
    let pdf_metadata: PdfStruct = fetch_metadata(&document, filepath);
    Some(pdf_metadata)
}

/// Validates metadata through an API call
fn get_api_metadata(pdf_obj : &PdfStruct, pdf_data : &mut PdfData) -> Result<PdfMetadata, Box<dyn Error>>{
    // If no title exist, no call is made
    if pdf_obj.assumed_title.is_empty() && pdf_obj.metadata_title.is_empty(){
        return Err("No title found in PDF".into());
    }
    
    // Make API call
    let runtime = match Runtime::new() {
        Ok(x) => x,
        Err(_) => {
            return Err("Failed to create runtime".into());
        }
    };

    match runtime.block_on(call(&pdf_obj, pdf_data)) {
        Ok(top_score) => {
            let Some(top) = top_score else {
                return Err("No API results".into());
            };
            
            // Result cutoff, if no results have a title confidence 70% or higher ignore the results
            if top.title_confidence >= 70.0 {
                return Ok(top);
            } else {
                return Err("Title from API call not close enough".into());
            }
        }
        Err(e) => {
            let err_msg = format!("Error retrieving metadata: {}", e);
            pdf_data.timeouts += 1;
            Err(err_msg.into())
        }
    }
}

/// Reads all PDF:s in given vector
pub fn read_pdf_dir(pdf_paths : &Vec<String>, pdf_data : &mut PdfData){
    for pdf_path in pdf_paths {
        println!("---");
        println!("{}", pdf_path);
        pdf_data.read += 1;

        match pdf_data.reader {
            0 => {
                tag_read_pdf(pdf_path, pdf_data);
            }
            1 => { 
                lo_read_pdf(pdf_path, pdf_data);
            }
            _ => {}
        }
    }
}

/// Reads a pdf with the tag-to-pdf library
fn tag_read_pdf(filepath: &str, pdf_data : &mut PdfData){
    match load_pdf_doc(filepath) {
        Ok(mut pdf) => {
            let mut pdf_meta = extract_metadata(&mut pdf, filepath);

            // Print title info
            if pdf_data.verbose == Verbose::Full {
                println!("MetaTitle = {}", pdf_meta.metadata_title);
                println!("AssumedTitle = {}", pdf_meta.assumed_title);
            }

            if !pdf_data.make_api_call {
                pdf_data.pdfs.push(pdf_meta);
                return;
            }

            // Make API Call
            match get_api_metadata(&pdf_meta, pdf_data) {
                Ok(x) => {
                    if pdf_data.verbose != Verbose::Light {
                        println!("Confidence score: {:.0}", x.title_confidence);
                    }
                    pdf_meta.api_metadata = Some(x);
                    pdf_data.api_hits += 1;

                }
                Err(e) => {
                    println!("{}", e);
                }
            };
            pdf_data.pdfs.push(pdf_meta);
        }
        Err(e) =>{
            println!("Pdf Loading Error: {:?}", e);
            pdf_data.fails += 1;
        }
    };
}

/// Reads a pdf with the lopdf library
fn lo_read_pdf(filepath: &str, pdf_data : &mut PdfData){
    // LOPDF
    match read_pdf_metadata(filepath) {
        Some(mut pdf_meta) => {
            if pdf_data.verbose == Verbose::Full {
                println!("MetaTitle = {}", pdf_meta.metadata_title);
                println!("AssumedTitle = {}", pdf_meta.assumed_title);
            }

            if !pdf_data.make_api_call {
                pdf_data.pdfs.push(pdf_meta);
                return;
            }

            // Make API Call
            if let Err(err) = get_api_metadata(&mut pdf_meta, pdf_data){
                println!("{}", err);
            } else {
                pdf_data.api_hits += 1;
            }
            pdf_data.pdfs.push(pdf_meta);
        },
        None => {
            println!("Failed to read pdf");
            pdf_data.fails += 1;
        }
    };
}
