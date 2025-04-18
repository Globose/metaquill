use std::thread::panicking;

use crate::{decoding::decode_pdfdoc, document::{skip_whitespace, Document, Reader}, encoding::PDFDOC_MAP, pdf_object::{cmp_u8, is_delimiter, parse_object, PdfParseError, PdfVar}, print_raw};

#[derive(Debug)]
pub struct TextReader {
    page : Reader,
}

#[derive(Debug, Clone)]
pub struct Text{
    pub pos_y : f64,
    pub scaled_font_size : f64,
    pub chars : Vec<u32>,
}

pub fn get_page_resources(doc : &mut Document, page_obj : &PdfVar){
    println!("Page res");
    // TODO: handle case where Resources is an indirect object?? Maybe in the method get_dict_value??

    let Some(mut resource_dict_obj) = page_obj.get_dict_value("Resources") else{
        println!("Failed to find resource dictionary");
        return;
    };

    // A bit stupid :/
    let font_dict_obj = match resource_dict_obj {
        PdfVar::IndirectObject(obj_id) => {
            let Ok(resource_dict) = doc.get_object_by_id(*obj_id) else{
                println!("Failed to get obj with id {}", obj_id);
                return;
            };
            resource_dict
        }
        PdfVar::Dictionary(_) => resource_dict_obj.clone(),
        _ => {
            println!("Resources dict has to be of type Dictionary or Indirect Object");
            return;
        }
    };

    let Some(font_dict_obj) = font_dict_obj.get_dict_value("Font") else{
        println!("Failed to find font");
        return;
    };

    let PdfVar::Dictionary(font_dictionary) = font_dict_obj else{
        println!("Font dictionary not dict");
        return;
    };
    println!("Font {:?}", font_dictionary);
    for (fkey, pdfvar) in font_dictionary{
        println!("Fkey {}", fkey);
        let Some(obj_id) = pdfvar.get_usize() else {
            println!("Failed to get font {}", fkey);
            continue;
        };
        
        let Ok(font_obj) = doc.get_object_by_id(obj_id) else {
            println!("Failed to find object {}", obj_id);
            continue;
        };

        println!("Fobj {:?}", font_obj);

        break;
    }
}

/// Returns a vector of text based on a list of content objects
pub fn read_objects_text(doc : &mut Document, obj_ids : Vec<usize>) -> Option<Vec<Text>>{
    let mut page_u8 : Vec<u8> = Vec::new();

    // Iterate over all content objects for the page, store eveything in One Vector
    for obj_id in obj_ids{
        let Ok(obj) = doc.get_object_by_id(obj_id) else{
            return None;
        };
        let Some(decoded) = obj.get_decoded_stream(&mut doc.reader) else {
            return None;
        };
        page_u8.extend(decoded);
    }

    let mut page = Reader{data : page_u8, it : 0};
    let mut text_objects : Vec<Text> = Vec::new();

    // print_raw(&page.data, 0, page.size());
    let mut text = Text{pos_y : -1.0, chars : Vec::new(), scaled_font_size : 0.0};

    while page.it < page.size() {
        // Find BT section
        while page.it < page.size() {
            if page.byte() != b'B' {
                page.it += 1;
                continue;
            }
            if cmp_u8(&page.data, page.it, b"BT"){
                page.it += 2;
                break;
            }
            while !is_delimiter(&page.data, page.it){
                page.it += 1;
            }
        }

        // println!("\n----");
        if page.it >= page.size(){
            break;
        }
        parse_text_section(&mut page, &mut text_objects, &mut text);
    }
    
    add_text_section(&mut text, &mut text_objects, 0.0, 0.0);
    Some(text_objects)
}

/// Parses a BT section reading all text elements
fn parse_text_section(page : &mut Reader, text_objects : &mut Vec<Text>, text : &mut Text){
    let mut stack : Vec<PdfVar> = Vec::new();
    let mut y_pos : f64 = 0.0;
    let mut scale : f64 = 1.0;
    let mut font_size : f64 = 1.0;
    let mut scaled_font_size : f64 = 1.0;
    let mut newline = false;
    let mut leading : f64 = 0.0;

    loop {
        page.skip_whitespace();
        match page.byte() {
            b'T' => {
                page.it += 1;
                match page.byte() {
                    b'f' => {
                        let Some(font_size_obj) = stack.get(1) else {
                            println!("Stack wrong Tf");
                            return;
                        };
                        let Some(font_size_tmp) = font_size_obj.get_f64() else{
                            println!("Font size not num");
                            return;
                        };

                        font_size = font_size_tmp;
                        scaled_font_size = font_size*scale;
                    }
                    b'J' => {
                        let Some(tj_obj) = stack.get(0) else{
                            println!("Stakc empty TJ");
                            return;
                        };
                        let PdfVar::Array(tj_array) = tj_obj else{
                            println!("TJ obj is not array");
                            return;
                        };

                        eval_text_section(text, text_objects, y_pos, scaled_font_size);

                        // Add the text to the text section
                        for pdfvar in tj_array{
                            if let Some(num) = pdfvar.get_f64(){
                                if num < -150.0 {
                                    text.chars.push(32);
                                }
                                continue;
                            }
                            if let PdfVar::StringLiteral(string_lit) = pdfvar {
                                text.chars.extend(string_lit);
                                let s = decode_pdfdoc(string_lit);
                                continue;
                            }
                            println!("Other {:?}", pdfvar);
                        }

                    }
                    b'm' => {
                        let Some(ty_obj) = stack.get(5) else{
                            println!("Tm stack faile");
                            return;
                        };
                        let Some(scale_obj) = stack.get(0) else{
                            return;
                        };

                        // Get values
                        let Some(ty) = ty_obj.get_f64() else{
                            return;
                        };
                        let Some(new_scale) = scale_obj.get_f64() else{
                            return;
                        };

                        scale = new_scale;
                        y_pos = ty;
                        scaled_font_size = font_size*scale;
                    }
                    b'd' | b'D' | b'*' => {
                        // TODO: space if tx > x
                        let mut ty_value = -leading;
                        if page.byte() != b'*'{
                            let Some(ty_obj) = stack.get(1) else{
                                println!("Td fail get 1");
                                return;
                            };
                            let Some(ty) = ty_obj.get_f64() else{
                                return;
                            };
                            ty_value = ty;
                        }
                        if page.byte() == b'D'{
                            leading = -ty_value;
                        }

                        // Set new value for y-pos, if it is a new BT section position is reset to 0 and then updated
                        let mut y_new = y_pos + ty_value * scale;
                        y_pos = y_new;
                    }
                    b'L' => {
                        let Some(l_obj) = stack.get(0) else{
                            println!("Td fail get L");
                            return;
                        };
                        let Some(lf) = l_obj.get_f64() else{
                            println!("lobj not f");
                            return;
                        };
                        leading = lf;
                    }
                    b'j' => {
                        let Some(str_obj) = stack.get(0) else{
                            println!("Faild string Tj");
                            return;
                        };

                        eval_text_section(text, text_objects, y_pos, scaled_font_size);

                        // Add text
                        let PdfVar::StringLiteral(string_lit) = str_obj else{
                            println!("Failed to conv strobj to strlit");
                            return;
                        };
                        text.chars.extend(string_lit);
                    }
                    b'c' | b'w' | b'z' | b'r' | b's' => {
                        // Ignore
                    }
                    _ => {
                        println!("Unmatched T{}", page.byte() as char);
                        println!("Stack {:?}", stack);
                    }
                }
                page.it +=1;
                
                stack.clear();
            }
            b'\'' => {
                println!("Single - quote ");
            }
            b'"' => {
                println!("Double quote");
            }
            b'E' => {
                if cmp_u8(&page.data, page.it, b"ET"){
                    page.it += 2;
                    break;
                }
                else{
                    let uktype = read_text(page);
                    stack.clear();
                }
            }
            _ => {
                if let Err(e) = parse_object(page, &mut stack){
                    match e {
                        PdfParseError::UnmatchedChar => {
                            let uktype = read_text(page);
                            // println!("uktype {}", uktype);
                            stack.clear();
                        }
                        _ => {
                            println!("Error other {:?}", e);
                            return;
                        }
                    }
                }
            }
        }
    }
    // let ss = decode_pdfdoc(&chars);
    // println!("t {}", ss);
    // println!("Stackc {:?}", stack);
}

/// Evaluates if a new text segment belongs to the current text section, creates a new text section otherwise
fn eval_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // Compare y-position of last text to the new one

    // println!("Eval {:?}, {}", text, y_pos);
    let diff = (text.pos_y-y_pos).abs();
    if diff > 3.0*text.scaled_font_size {
        // New Text section
        add_text_section(text, text_objects, y_pos, scaled_font_size);
    } else if diff > 1.0*text.scaled_font_size {
        // New row, look if fontsize has changed
        if (text.scaled_font_size-scaled_font_size).abs() > 0.2{
            add_text_section(text, text_objects, y_pos, scaled_font_size);
        }
        else{
            // Update the y-value of the text segment
            text.chars.push(32);
            text.pos_y = y_pos;
        }
    }
}

fn add_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // New text section
    if text.chars.len() > 0{
        // Save previous text segment when new is found
        let text_obj = Text{pos_y : text.pos_y, scaled_font_size : text.scaled_font_size, chars : text.chars.clone()};
        text_objects.push(text_obj);
        text.chars.clear();
    }
    text.pos_y = y_pos;
    text.scaled_font_size = scaled_font_size;
}

/// Reads all ascii chars until something else
fn read_text(rd : &mut Reader) -> String{
    let mut output = String::new();
    loop {
        if rd.byte().is_ascii_alphabetic(){
            output.push(rd.byte() as char);
            rd.it += 1;
        }
        else if is_delimiter(&rd.data, rd.it){
            break;
        } else{
            output.push(rd.byte() as char);
            rd.it += 1;    
        }
    }

    output
}

