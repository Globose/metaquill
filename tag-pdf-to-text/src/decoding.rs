use std::io::Read;

use crate::{document::{Document, PdfError}, pdf_object::PdfVar, PDFDOC_MAP};
use flate2::read::ZlibDecoder;

/// Decodes a u32 char to a String
pub fn decode_pdfdoc_char(byte : u32) -> String{
    if !(32..254).contains(&byte){
        return String::new();
    }
    let Some(chr) = PDFDOC_MAP.get(byte as usize) else{
        return String::new();
    };
    if *chr != '\0'{
        return chr.to_string();
    }
    return String::new();
}

/// Decodes the PDFDOC text encoding
pub fn decode_pdfdoc(bytes : &Vec<u32>) -> String{
    let mut decoded = String::new();
    for byte in bytes{
        if !(32..254).contains(byte){
            continue;
        }
        let Some(chr) = PDFDOC_MAP.get(*byte as usize) else{
            continue;
        };
        if *chr != '\0'{
            decoded.push(*chr);
        }
    }
    return decoded;
}

/// PNG-decoding, returns decoded stream
pub fn png_decode(stream: &Vec<u8>, _predictor: usize, columns: usize) -> Result<Vec<u8>, PdfError>{
    let mut i : usize = 0;
    let mut decoded : Vec<u8> = Vec::new();

    while i < stream.len() {
        // Get filter type
        let filter_type = stream[i];
        i += 1;

        // Break if data ends
        if i + columns > stream.len() {
            break;
        }

        match filter_type { 
            2 => {
                let dec_len = decoded.len() as i32; 
                for j in 0..columns {
                    let top_index : i32 = dec_len-(columns as i32)+(j as i32);
                    if top_index >= 0{
                        decoded.push(stream[j+i].wrapping_add(decoded[top_index as usize]));
                    }
                    else{
                        decoded.push(stream[j+i]);
                    }
                }
            }
            _ => {
                // Unsupported filter type
                return Err(PdfError::DecodeError);
            }
        }

        i += columns;
    }

    return Ok(decoded);
}

/// Decode flate stream in place (functionally)
pub fn decode_flate(data : &mut Vec<u8>) -> Result<(), PdfError>{
    let data_copy = data.clone();
    data.clear();
    let mut decoder = ZlibDecoder::new(data_copy.as_slice());
    
    // Attempt to decode flate
    match decoder.read_to_end(data){
        Ok(_) => Ok(()),
        Err(e) => {
            println!("Failed to decompress zlib: {}", e);
            Err(PdfError::DecodeError)
        }
    }
}

/// Processes a decoded stream based on the decodeparms
pub fn handle_decodeparms(stream : Vec<u8>, decodeparms_obj : &PdfVar, doc : &mut Document) -> Result<Vec<u8>, PdfError>{
    let mut predictor : usize = 1;
    let mut columns : usize = 1;

    // Fetch Predictor value
    if let Some(pred_obj) = decodeparms_obj.get_dict_value("Predictor"){
        if let Some(pred_usize) = pred_obj.get_usize(doc) {
            predictor = pred_usize;
        };
    };
    
    // Fetch Columns value
    if let Some(columns_obj) = decodeparms_obj.get_dict_value("Columns"){
        if let Some(columns_usize) = columns_obj.get_usize(doc) {
            columns = columns_usize;
        };
    };

    // Handle PNG-decoding
    if (10..16).contains(&predictor){
        return png_decode(&stream, predictor, columns);
    }

    return Err(PdfError::DecodeError);
}


/// Converts base 256 to decimal
pub fn get_256_repr(bytes : &[u8]) -> usize{
    let mut output : usize = 0;
    for &byte in bytes{
        output = output * 256 + (byte as usize);
    }
    return output;
}
