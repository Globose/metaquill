use std::error::Error;
use std::path::Path;
use lopdf::Document;
use tag_pdf_to_text::load_pdf_doc;
use tokio::runtime::Runtime;
use crate::file_manager::load_pdf;
use crate::metadata::{extract_metadata, fetch_metadata, PdfStruct};
use crate::call::call;
use crate::PdfData;

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
fn get_api_metadata(pdf_obj : &mut PdfStruct, pdf_data : &PdfData) -> Result<(), Box<dyn Error>>{
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
                pdf_obj.api_metadata = Some(top);
                return Ok(());
            } else {
                return Err("Title from API call not close enough".into());
            }
        }
        Err(e) => {
            let err_msg = format!("Error retrieving metadata: {}", e);
            Err(err_msg.into())
        }
    }
}

/// Reads all PDF:s in given folder, reads a Pdf if path is a pdf
pub fn read_pdf_dir(path: &Path, pdf_data : &mut PdfData) -> Option<()>{
    if path.is_dir(){
        // If path is a directory
        let Ok(entries) = std::fs::read_dir(path) else{
            return None;
        };
        for entry in entries {
            let Ok(ent) = entry else{
                continue;
            };
            let ent_path = ent.path();
            read_pdf_dir(&ent_path, pdf_data);
        }
    }
    else{
        // If path is a file
        let Some(extension) = path.extension() else {
            return None;
        };

        // Only care when file extension is .pdf
        if extension != "pdf" {
            return None;
        }
        let file_path_str = path.to_str().unwrap_or("").to_string();
        read_pdf(&file_path_str, pdf_data);
        // export_json(&mut pdf_data.pdfs); // ALTERNATIVE
    }
    return None;
}

/// Reads one PDF document and adds the result to the result vector
pub fn read_pdf(filepath: &str, pdf_data : &mut PdfData){
    println!("---");
    println!("{}", filepath);
    pdf_data.read += 1;

    match pdf_data.reader {
        0 => {
            tag_read_pdf(filepath, pdf_data);
        }
        1 => { 
            lo_read_pdf(filepath, pdf_data);
        }
        _ => {}
    }

}

/// Reads a pdf with the tag-to-pdf library
fn tag_read_pdf(filepath: &str, pdf_data : &mut PdfData){
    match load_pdf_doc(filepath) {
        Ok(mut pdf) => {
            let mut pdf_meta = extract_metadata(&mut pdf, filepath);

            // Print title info
            if pdf_data.print_info {
                println!("MetaTitle = {}", pdf_meta.metadata_title);
                println!("AssumedTitle = {}", pdf_meta.assumed_title);
            }

            if !pdf_data.make_api_call {
                pdf_data.pdfs.push(pdf_meta);
                return;
            }

            // Make API Call
            if let Err(err) = get_api_metadata(&mut pdf_meta, &pdf_data){
                if pdf_data.print_info{
                    println!("{}", err);
                }
            } else {
                pdf_data.api_hits += 1;
            }
            pdf_data.pdfs.push(pdf_meta);
        }
        Err(e) =>{
            println!("Err: {:?}", e);
            pdf_data.fails += 1;
        }
    };
}

/// Reads a pdf with the lopdf library
fn lo_read_pdf(filepath: &str, pdf_data : &mut PdfData){
    // LOPDF
    match read_pdf_metadata(filepath) {
        Some(mut pdf_meta) => {
            if pdf_data.print_info {
                println!("MetaTitle = {}", pdf_meta.metadata_title);
                println!("AssumedTitle = {}", pdf_meta.assumed_title);
            }

            if !pdf_data.make_api_call {
                pdf_data.pdfs.push(pdf_meta);
                return;
            }

            // Make API Call
            if let Err(err) = get_api_metadata(&mut pdf_meta, &pdf_data){
                if pdf_data.print_info{
                    println!("{}", err);
                }
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

    match load_pdf_doc(filepath) {
        Ok(mut pdf) => {
            let mut pdf_meta = extract_metadata(&mut pdf, filepath);

            // Print title info
            if pdf_data.print_info {
                println!("MetaTitle = {}", pdf_meta.metadata_title);
                println!("AssumedTitle = {}", pdf_meta.assumed_title);
            }

            if !pdf_data.make_api_call {
                pdf_data.pdfs.push(pdf_meta);
                return;
            }

            // Make API Call
            if let Err(err) = get_api_metadata(&mut pdf_meta, &pdf_data){
                if pdf_data.print_info{
                    println!("{}", err);
                }
            } else {
                pdf_data.api_hits += 1;
            }
            pdf_data.pdfs.push(pdf_meta);
        }
        Err(e) =>{
            println!("Err: {:?}", e);
            pdf_data.fails += 1;
        }
    };
}
