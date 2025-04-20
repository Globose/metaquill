use std::io::{BufRead, BufReader, Cursor, Read};

use crate::{document::{Document}, pdf_object::PdfVar, PDFDOC_MAP};
use flate2::read::ZlibDecoder;
use lzw::{Decoder, DecoderEarlyChange, LsbReader};

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

/// Decodes the PDFDOC text encoding
pub fn decode_pdfdoc_u8(bytes : &Vec<u8>) -> String{
    let mut decoded = String::new();
    for byte in bytes{
        if *byte == 13 || *byte == 10{
            decoded.push('\n');
        }
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
pub fn png_decode(stream: &Vec<u8>, _predictor: usize, columns: usize) -> Option<Vec<u8>>{
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
                println!("Unsupported filter type: {}", filter_type);
                return None;
            }
        }

        i += columns;
    }

    return Some(decoded);
}

/// Decode flate stream
pub fn decode_flate(doc : &mut Document, start : usize, size : usize) -> Option<Vec<u8>>{
    doc.it = start;
    let compressed_data = &doc.data[doc.it..doc.it + size];
    
    let mut decoder = ZlibDecoder::new(compressed_data);
    let mut output = Vec::new();
    
    // Attempt to decode flate
    match decoder.read_to_end(&mut output){
        Ok(_) => Some(output),
        Err(e) => {
            println!("Failed to decompress zlib: {}", e);
            None
        }
    }
}

/// Processes a decoded stream based on the decodeparms
pub fn handle_decodeparms(stream : Vec<u8>, decodeparms_obj : &PdfVar, doc : &mut Document) -> Option<Vec<u8>>{
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
        return match png_decode(&stream, predictor, columns){
            Some(x) => Some(x),
            None => None,
        };
    }

    println!("Unknown Predictor Value {}", predictor);
    return None;
}


/// Converts base 256 to decimal
pub fn get_256_repr(bytes : &[u8]) -> usize{
    let mut output : usize = 0;
    for &byte in bytes{
        output = output * 256 + (byte as usize);
    }
    return output;
}

/// Decodes a LZW stream
pub fn decode_lzw(doc : &mut Document, start : usize, size : usize, obj_dict : &PdfVar) -> Option<Vec<u8>> {
    println!("DECODE LZW");

    return None;
}