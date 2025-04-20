use std::{collections::HashMap, hash::Hash, thread::panicking};

use crate::{decoding::{decode_pdfdoc, decode_pdfdoc_char, decode_pdfdoc_u8}, document::{skip_whitespace, Document}, encoding::PDFDOC_MAP, pdf_object::{cmp_u8, is_delimiter, parse_object, to_hex, PdfParseError, PdfVar}, print_raw};

#[derive(Debug, Clone)]
pub struct Text{
    pub pos_y : f64,
    pub scaled_font_size : f64,
    pub chars : String,
    font : String,
}

#[derive(Debug)]
pub struct Font{
    name : String,
    mapping : HashMap<u32,Vec<u32>>,
}

/// Reads the unicode Char Mappings for the fonts on the page
pub fn get_page_resources(doc : &mut Document, page_obj : &PdfVar) -> Vec<Font>{
    // TODO: handle case where Resources is an indirect object?? Maybe in the method get_dict_value??
    let mut fonts : Vec<Font> = Vec::new();
    fonts.push(Font{name : String::new(), mapping : HashMap::new()});
    
    let Some(mut resource_dict_obj) = page_obj.get_dict_value("Resources") else{
        println!("Failed to find resource dictionary");
        return fonts;
    };
    
    // Retrieve Resources object from ID
    let font_dict_obj = match resource_dict_obj {
        PdfVar::IndirectObject(obj_id) => {
            let Ok(resource_dict) = doc.get_object_by_id(*obj_id) else{
                println!("Failed to get obj with id {}", obj_id);
                return fonts;
            };
            resource_dict
        }
        PdfVar::Dictionary(_) => resource_dict_obj.clone(),
        _ => {
            println!("Resources dict has to be of type Dictionary or Indirect Object");
            return fonts;
        }
    };

    // Read Object Member Font-object
    let Some(font_dict) = font_dict_obj.get_dict_value("Font") else{
        println!("Failed to find font");
        return fonts;
    };

    let all_fonts = match font_dict {
        PdfVar::Dictionary(x) => x.clone(),
        PdfVar::IndirectObject(x) => {
            let Ok(unpacked_indirect_obj) = doc.get_object_by_id(*x) else {
                return fonts;
            };
            let PdfVar::Dictionary(font_dict) = unpacked_indirect_obj else {
                return fonts;
            };
            font_dict
        }
        _ => {
            println!("Unknown font dict");
            return fonts;
        }
    };

    // Read the Font Object as a dictionary
    let PdfVar::Dictionary(font_dictionary) = font_dict else{
        println!("Font dictionary not dict");
        return fonts;
    };

    // Read all fonts
    for (fkey, pdfvar) in font_dictionary{
        // Get Object ID for the given font
        let Some(obj_id) = pdfvar.get_indirect_obj_index(doc) else {
            println!("Failed to get font {}", fkey);
            continue;
        };
        
        // Retrieve the object with the ID
        let Ok(font_obj) = doc.get_object_by_id(obj_id) else {
            println!("Failed to find object {}", obj_id);
            continue;
        };

        // Retrieve a ToUnicode
        let Some(to_unicode_id) = font_obj.get_dict_int("ToUnicode", doc) else{
            // println!("Font {} contains no ToUnicode", fkey);
            continue;
        };

        // Fetch ToUnicode Object
        let Ok(to_unicode_obj) = doc.get_object_by_id(to_unicode_id) else{
            println!("Failed to fetch ToUnicode object");
            continue;;
        };
        // println!("ToUnicodeObject {:?}", to_unicode_obj);
        let Some(to_unicode_content) = to_unicode_obj.get_decoded_stream(doc) else{
            println!("Failed to Unpack ToUnicode");
            continue;;
        };
        
        let mut codex : HashMap<u32, Vec<u32>> = HashMap::new(); 
        // let mut char_reader = Reader{data : to_unicode_content, it : 0};
        
        // Add decoded to document
        doc.it = doc.size();
        doc.data.extend(to_unicode_content);

        loop {
            // Exit when everything is covered
            if doc.it >= doc.size(){
                break;
            }

            // Search for unicode mappings
            if doc.byte() == b'b' {
                if cmp_u8(&doc.data, doc.it, b"beginbfchar"){
                    doc.it += 11;
                    read_fchar(doc, &mut codex);
                }
                if cmp_u8(&doc.data, doc.it, b"beginbfrange"){
                    doc.it += 12;
                    read_frange(doc, &mut codex);
                }
            }
            doc.it += 1;
        }
        fonts.push(Font{name : fkey.to_string(), mapping : codex});
    }
    fonts
}

/// Reads key-value pairs from beginbfrange-section in ToUnicode, and adds them to the translation map
fn read_frange(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>) -> Option<()>{
    // print_raw(&rd.data, rd.it, rd.size());
    loop {
        // Go to next
        let mut char_range: [u32; 2] = [0,0];
        for i in 0..2{
            doc.skip_whitespace();
            if doc.byte() != b'<' {
                return Some(());
            }
            doc.it += 1;
            
            // Read the hex-char, can be 2-4 chars
            let mut hex_str : Vec<u8> = Vec::new();
            while doc.byte().is_ascii_alphanumeric() {
                hex_str.push(doc.byte());
                doc.it += 1;
            }

            // Get range param
            let Ok(value) = to_hex(&hex_str) else {
                println!("Error: Range");
                return None;
            };

            if doc.byte() != b'>' {
                println!("No > at end");
                return None;
            }
            char_range[i] = value;
            doc.it += 1;
        }

        doc.skip_whitespace();
        // println!("Range {:?}", char_range);
        let mut ix = char_range[0];
        // Read mapping
        if doc.byte() == b'['{
            // Array mapping
            doc.it += 1;
            loop {
                doc.skip_whitespace();
                if doc.byte() == b']' {
                    break;
                }
                let Some(v) = read_hex_chars(doc) else {
                    println!("Error Array mapping");
                    return None;
                };
                codex.insert(ix, v);
                ix += 1;
            }
        } else if doc.byte() == b'<' {
            // Range from number mapping
            doc.it += 1;
            doc.skip_whitespace();
            let Ok(value) = to_hex(&doc.data[doc.it..doc.it+4]) else {
                println!("Range number mapping error");
                return None;
            };
            doc.it += 4;
            if doc.byte() != b'>' {
                println!("End Range mapping error >");
                return None;
            }
            doc.it += 1;

            for i in 0..char_range[1]-char_range[0]+1{
                codex.insert(i+char_range[0], vec![value+i]);
            }

        } else {
            println!("Failed to find mapping {}", doc.byte() as char);
            return None;
        }
    }
}

/// Reads key-value pairs from beginbfchar-section in ToUnicode, and adds them to the translation map
fn read_fchar(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>) -> Option<()>{
    // print_raw(&rd.data, rd.it, rd.data.len());
    loop {
        doc.skip_whitespace();
        if doc.byte() != b'<' {
            return None;
        }
        doc.it += 1;
        
        // Read the hex-char, can be 2-4 chars
        let mut hex_str : Vec<u8> = Vec::new();
        while doc.byte().is_ascii_alphanumeric() {
            hex_str.push(doc.byte());
            doc.it += 1;
        }

        // Get the key value
        let Ok(key) = to_hex(&hex_str) else {
            println!("No Key");
            return None;
        };
        
        if doc.byte() != b'>' {
            println!("No >");
            return None; 
        }
        doc.it += 1;
        
        // Get All values for the key
        // println!("Preval {}", rd.it);
        if let Some(values) = read_hex_chars(doc){
            // println!("Val {:?}", values);
            codex.insert(key, values);
        }
        // println!("post {}", rd.it);
    }
    return Some(());
}

/// Reads a hex-string <4*k>, returns u32 vector
fn read_hex_chars(doc : &mut Document) -> Option<Vec<u32>>{
    doc.skip_whitespace();
    if doc.byte() != b'<'{
        return None;
    }
    doc.it += 1;

    let mut chars : Vec<u32> = Vec::new();
    loop {
        let Ok(num) = to_hex(&doc.data[doc.it..doc.it+4]) else {
            println!("Failed to get hex repr");
            return None;
        };
        chars.push(num);
        doc.it += 4;
        if doc.byte() == b'>'{
            break;
        }
    }
    doc.it += 1;
    return Some(chars);
}

/// Returns a vector of text based on a list of content objects
pub fn read_objects_text(doc : &mut Document, obj_ids : Vec<usize>, fonts : &Vec<Font>) -> Option<Vec<Text>>{
    let mut page_u8 : Vec<u8> = Vec::new();
    // Iterate over all content objects for the page, store eveything in One Vector
    let start = doc.size();
    for obj_id in obj_ids{
        let Ok(obj) = doc.get_object_by_id(obj_id) else{
            return None;
        };
        let Some(decoded) = obj.get_decoded_stream(doc) else {
            continue;
            // return None;
        };
        doc.data.extend(decoded);
    }

    doc.it = start;
    let mut text_objects : Vec<Text> = Vec::new();
    let mut text: Text = Text{pos_y : -1.0, chars : String::new(), scaled_font_size : 0.0, font : String::new()};

    while doc.it < doc.size() {
        // Find BT section
        while doc.it < doc.size() {
            if doc.byte() != b'B' {
                doc.it += 1;
                continue;
            }
            if cmp_u8(&doc.data, doc.it, b"BT"){
                doc.it += 2;
                break;
            }
            while !is_delimiter(&doc.data, doc.it){
                doc.it += 1;
            }
        }

        if doc.it >= doc.size(){
            break;
        }
        parse_text_section(doc, &mut text_objects, &mut text, &fonts);
    }
    
    add_text_section(&mut text, &mut text_objects, 0.0, 0.0);
    Some(text_objects)
}

/// Parses a BT section reading all text elements
fn parse_text_section(doc : &mut Document, text_objects : &mut Vec<Text>, text : &mut Text, fonts : &Vec<Font>){
    let mut stack : Vec<PdfVar> = Vec::new();
    let mut y_pos : f64 = 0.0;
    let mut scale : f64 = 1.0;
    let mut font_size : f64 = 1.0;
    let mut scaled_font_size : f64 = text.scaled_font_size;
    let mut newline = false;
    let mut leading : f64 = 0.0;

    loop {
        doc.skip_whitespace();
        match doc.byte() {
            b'T' => {
                doc.it += 1;
                match doc.byte() {
                    b'f' => {
                        let Some(font_name_obj) = stack.get(0) else{
                            println!("Stack wrong tf 0");
                            return;
                        };
                        let Some(font_name) = font_name_obj.get_name() else{
                            println!("Failed to get font name");
                            return;
                        };
                        let Some(font_size_obj) = stack.get(1) else {
                            println!("Stack wrong Tf");
                            return;
                        };
                        let Some(font_size_tmp) = font_size_obj.get_f64() else{
                            println!("Font size not num");
                            return;
                        };

                        text.font = font_name;
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
                                if num < -165.0 {
                                    text.chars.push(' ');
                                }
                                continue;
                            }
                            if let PdfVar::StringLiteral(string_lit) = pdfvar {
                                // text.chars.extend(string_lit);
                                add_str_lit(text, string_lit, fonts);
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

                        // Sets value to -leading at the start
                        // If Td or TD, the value should be replaced with an arg
                        let mut ty_value = -leading;
                        let mut tx : f64 = 0.0;
                        if doc.byte() != b'*'{
                            let Some(ty_obj) = stack.get(1) else{
                                println!("Td fail get 1");
                                return;
                            };
                            let Some(ty) = ty_obj.get_f64() else{
                                return;
                            };

                            // Fetch x-move-value
                            let Some(tx_obj) = stack.get(0) else {
                                return;
                            };
                            if let Some(tx_temp) = tx_obj.get_f64(){
                                tx = tx_temp;
                            };
                            ty_value = ty;
                        }

                        // For TD, modify leading value
                        if doc.byte() == b'D'{
                            leading = -ty_value;
                        }

                        // If x-move is large, we have a space
                        if tx > 150.0{
                            // println!("GAP");
                            text.chars.push(' ');
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
                        add_str_lit(text, string_lit, fonts);
                        // text.chars.extend(string_lit);
                    }
                    b'c' | b'w' | b'z' | b'r' | b's' => {
                        // Ignore
                    }
                    _ => {
                        println!("Unmatched T{}", doc.byte() as char);
                        println!("Stack {:?}", stack);
                    }
                }
                doc.it +=1;
                
                stack.clear();
            }
            b'\'' => {
                println!("Single - quote ");
                return;
            }
            b'"' => {
                println!("Double quote");
                return;
            }
            b'E' => {
                if cmp_u8(&doc.data, doc.it, b"ET"){
                    doc.it += 2;
                    break;
                }
                else{
                    let uktype = read_text(doc);
                    stack.clear();
                }
            }
            _ => {
                if let Err(e) = parse_object(doc, &mut stack){
                    match e {
                        PdfParseError::UnmatchedChar => {
                            let uktype = read_text(doc);
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
}

fn add_str_lit(text : &mut Text, string_lit : &Vec<u32>, fonts : &Vec<Font>){
    // Fetch font
    let mut font : &Font = &fonts[0];
    for f in fonts{
        if f.name == text.font{
            font = f;
            break;
        }
    }

    // let mut s = String::new();
    // Iterate over all chars
    for key in string_lit{
        if *key == 0{
            continue;
        }
        // println!("Font-- {:?}", font);
        // let u = font.mapping.contains_key(key);
        // println!("Contains {}", u);
        let Some(x_vec) = font.mapping.get(key) else {
            text.chars.push_str(decode_pdfdoc_char(*key).as_str());
            // println!("Char1 {}, {}", key, decode_pdfdoc_char(*key).as_str());
            continue;
        };
        for x in x_vec{
            let Some(uc) = char::from_u32(*x) else {
                // println!("Failed to map {}", x);
                continue;
            };
            text.chars.push(uc);
            // text.chars.push_str(&decode_pdfdoc_char(*x).as_str());
            // println!("Char {}, {}", key, uc);
            // s.push_str(&decode_pdfdoc_char(*x).as_str());
        }
    }
    // println!("String {}", s);
}

/// Evaluates if a new text segment belongs to the current text section, creates a new text section otherwise
fn eval_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // Compare y-position of last text to the new one
    let diff = (text.pos_y-y_pos).abs();
    // println!("Diff {}", diff);
    // println!("Scfz {}", scaled_font_size);
    if diff > 3.0*text.scaled_font_size {
        // New Text section
        add_text_section(text, text_objects, y_pos, scaled_font_size);
    } else if diff > 0.7*text.scaled_font_size {
        // New row, look if fontsize has changed
        if (text.scaled_font_size-scaled_font_size).abs() > 0.2{
            add_text_section(text, text_objects, y_pos, scaled_font_size);
        }
        else{
            // Update the y-value of the text segment
            text.chars.push(' ');
            text.pos_y = y_pos;
        }
    }
}

fn add_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // New text section
    if text.chars.len() > 0{
        // Save previous text segment when new is found
        let text_obj = Text{pos_y : text.pos_y, scaled_font_size : text.scaled_font_size, chars : text.chars.clone(), font : String::new()};
        text_objects.push(text_obj);
        text.chars.clear();
    }
    text.pos_y = y_pos;
    text.scaled_font_size = scaled_font_size;
}

/// Reads all ascii chars until something else
fn read_text(doc : &mut Document) -> String{
    let mut output = String::new();
    loop {
        if doc.byte().is_ascii_alphabetic(){
            output.push(doc.byte() as char);
            doc.it += 1;
        }
        else if is_delimiter(&doc.data, doc.it){
            break;
        } else{
            output.push(doc.byte() as char);
            doc.it += 1;    
        }
    }

    output
}

