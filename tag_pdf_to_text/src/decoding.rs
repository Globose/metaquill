use std::char;
use std::io::Read;

use crate::document::{Document, PdfError};
use crate::encoding::PDFDOC_MAP;
use crate::pdf_object::PdfVar;
use flate2::read::ZlibDecoder;

/// Decodes a u32 char to a String
pub(crate) fn decode_pdfdoc_char(byte : u32) -> String{
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
pub(crate) fn decode_pdfdoc(bytes : &Vec<u32>) -> String{
    let mut decoded = String::new();
    
    // Look for 254 255, UTF16BE
    let Some(b0) = bytes.get(0) else {
        return decoded;
    };
    if let Some(b1) = bytes.get(1) {
        if *b0 == 254 && *b1 == 255{
            let mut ix = 3;
            while ix < bytes.len(){
                let d0 = bytes[ix-1] as u8;
                let d1 = bytes[ix] as u8;
                let code = u16::from_be_bytes([d0,d1]);
                if let Some(ch) = char::from_u32(code as u32) {
                    decoded.push(ch);
                }
                ix += 2;
            }
            return decoded;
        }
    }

    // Not special hex
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
pub(crate) fn png_decode(stream: &Vec<u8>, _predictor: usize, columns: usize) -> Result<Vec<u8>, PdfError>{
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
pub(crate) fn decode_flate(data : &mut Vec<u8>) -> Result<(), PdfError>{
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
pub(crate) fn handle_decodeparms(stream : Vec<u8>, decodeparms_obj : &PdfVar, doc : &mut Document) -> Result<Vec<u8>, PdfError>{
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
pub(crate) fn get_256_repr(bytes : &[u8]) -> usize{
    let mut output : usize = 0;
    for &byte in bytes{
        output = output * 256 + (byte as usize);
    }
    return output;
}
