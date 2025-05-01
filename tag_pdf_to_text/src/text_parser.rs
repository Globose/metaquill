use std::collections::HashMap;
use std::sync::Arc;
use std::vec;
use crate::pdf_object::{cmp_u8, is_delimiter, obj_parse_numeric, parse_object, to_hex, PdfVar};
use crate::document::{Document, PdfError};
use crate::decoding::decode_pdfdoc_char;
use crate::print_raw;

#[derive(Debug, Clone)]
pub struct Text{
    pub pos_y : f64,
    pub scaled_font_size : f64,
    pub chars : String,
    pub avg_font_size : f64,
    font : String,
}

impl Text {
    // Adds a space, given that the last char is not a space
    fn add_space(&mut self){
        if let Some(c0) = self.chars.chars().last(){
            if c0 == ' '{
                return;
            }
        }
        self.chars.push(' ');
    }
}

#[derive(Debug)]
pub(crate) struct Font{
    name : String,
    mapping : HashMap<u32,Vec<u32>>,
}

// 9 Params for text state params (p. 243)
#[derive(Debug, Clone)]
pub(crate) struct TextReader {
    y_pos : f64,
    scale : f64,
    graph_scale : f64,
    graph_y : f64,
    font_size : f64,
    scaled_font_size : f64,
    leading : f64,
}

/// Reads the unicode Char Mappings for the fonts on the page
/// The first font in the result vector is always an empty font
pub(crate) fn get_page_resources(doc : &mut Document, page_obj : &PdfVar) -> Vec<Font>{
    let mut fonts : Vec<Font> = Vec::new();
    fonts.push(Font{name : String::new(), mapping : HashMap::new()});

    // Find resources dictionary
    let Some(resource_dict_obj) = page_obj.get_dict_value("Resources") else{
        return fonts;
    };
    
    // Retrieve Resources object from ID
    let font_dict_obj = match resource_dict_obj {
        PdfVar::IndirectObject(obj_id) => {
            let Some(resource_dict) = doc.get_object_by_id(*obj_id) else{
                return fonts;
            };
            resource_dict
        }
        PdfVar::Dictionary(_) => resource_dict_obj.clone(),
        _ => {
            return fonts;
        }
    };

    // Read Object Member Font-object
    let Some(font_dict) = font_dict_obj.get_dict_value("Font") else{
        return fonts;
    };

    let all_fonts = match font_dict {
        PdfVar::Dictionary(x) => {
            x.clone()
        }
        PdfVar::IndirectObject(x) => {
            // Fetch the indirect object
            let Some(unpacked_indirect_obj) = doc.get_object_by_id(*x) else {
                return fonts;
            };
            let PdfVar::Object { _id, content } = unpacked_indirect_obj else {
                return fonts;
            };
            
            // Get the dictionary
            let Some(content_1) = content.get(1) else {
                return fonts;
            };

            let PdfVar::Dictionary(dict) = content_1 else {
                return fonts;
            };
            dict.clone()
        }
        _ => {
            return fonts;
        }
    };

    // Read all fonts
    for (fkey, pdfvar) in all_fonts{ 
        // Get Object ID for the given font
        let Some(obj_id) = pdfvar.get_indirect_obj_index() else {
            continue;
        };
        
        // Retrieve the object with the ID
        let Some(font_obj) = doc.get_object_by_id(obj_id) else {
            continue;
        };
        
        let mut codex : HashMap<u32, Vec<u32>> = HashMap::new(); 

        // Retrieve a ToUnicode
        if let Some(to_unicode_id) = font_obj.get_dict_int("ToUnicode"){
            read_to_unicode(doc, &mut codex, to_unicode_id);
        };

        // Retrieve encoding map
        if let Some(encoding_ref) = font_obj.get_dict_value("Encoding") {
            read_encoding(doc, &mut codex, encoding_ref);
        };

        fonts.push(Font{name : fkey.to_string(), mapping : codex});
    }
    fonts
}

/// Fetches encoding information
fn read_encoding(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>, encoding_ref : &PdfVar) {
    let Some(enc_id) = encoding_ref.get_indirect_obj_index() else {
        return;
    };
    let Some(enc_obj) = doc.get_object_by_id(enc_id) else {        
        return;
    };
    let Some(diff_obj) = enc_obj.get_dict_value("Differences") else {
        return;
    };
    let PdfVar::Array(enc_array) = diff_obj else {
        return;
    };

    // Get first index
    let Some(obj_0) = enc_array.get(0) else {
        return;
    };
    let Some(offset) = obj_0.get_usize(doc) else {
        return;
    };

    for i in 1..enc_array.len() {
        // enc_array contains many Name-objects -> C47, C99...
        let Some(name_obj) = enc_array.get(i) else {
            continue;
        };
        let Some(name) = name_obj.get_name() else {
            continue;
        };
        let Some(c1) = name.chars().next() else {
            continue;
        };
        if c1 != 'C'{
            continue;
        }

        let slice: String = name.chars().skip(1).collect();
        let Ok(map_value) = slice.parse::<u32>() else {
            continue;
        };
        let key= (i+offset-1) as u32;
        codex.insert(key, vec![map_value]);
    }

}

/// Parses the ToUnicode object for a font
fn read_to_unicode(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>, to_unicode_id : usize){
    // Fetch ToUnicode Object
    let Some(to_unicode_obj) = doc.get_object_by_id(to_unicode_id) else{
        return;
    };

    let Some(to_unicode_content) = to_unicode_obj.get_decoded_stream(doc) else{
        return;
    };
    
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
                read_fchar(doc, codex);
            }
            if cmp_u8(&doc.data, doc.it, b"beginbfrange"){
                doc.it += 12;
                read_frange(doc, codex);
            }
        }
        doc.it += 1;
    }
}

/// Reads key-value pairs from beginbfrange-section in ToUnicode, and adds them to the translation map
fn read_frange(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>) -> Option<()>{
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
                return None;
            };

            if doc.byte() != b'>' {
                return None;
            }
            char_range[i] = value;
            doc.it += 1;
        }

        doc.skip_whitespace();

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
                return None;
            };
            doc.it += 4;
            if doc.byte() != b'>' {
                return None;
            }
            doc.it += 1;

            for i in 0..char_range[1]-char_range[0]+1{
                codex.insert(i+char_range[0], vec![value+i]);
            }

        } else {
            return None;
        }
    }
}

/// Reads key-value pairs from beginbfchar-section in ToUnicode, and adds them to the translation map
fn read_fchar(doc : &mut Document, codex : &mut HashMap<u32, Vec<u32>>) -> Option<()>{
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
            return None;
        };
        
        if doc.byte() != b'>' {
            return None; 
        }
        doc.it += 1;
        
        // Get All values for the key
        if let Some(values) = read_hex_chars(doc){
            codex.insert(key, values);
        }
    }
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
            return None;
        };
        chars.push(num);
        doc.it += 4;
        doc.skip_whitespace();
        if doc.byte() == b'>'{
            break;
        }
    }
    doc.it += 1;
    return Some(chars);
}

/// Returns a vector of text based on a list of content objects
pub(crate) fn read_objects_text(doc : &mut Document, obj_ids : Vec<usize>, fonts : &Vec<Font>) -> Option<Vec<Text>>{
    // Iterate over all content objects for the page, store eveything in One Vector
    let start = doc.size();

    for obj_id in obj_ids{
        let Some(obj) = doc.get_object_by_id(obj_id) else{
            return None;
        };

        // Content can be either an array or a dictionary
        let PdfVar::Object{_id, content} = &obj else {
            return None;
        };

        let Some(obj_1) = content.get(1) else {
            return None;
        };

        if let Some(array) = obj_1.get_usize_array() {
            for index in array{
                let Some(objx) = doc.get_object_by_id(index) else {
                    return None;
                };
                let Some(decoded) = objx.get_decoded_stream(doc) else {
                    return None;
                };
                doc.data.extend(decoded);
            }
            continue;
        }

        let Some(decoded) = obj.get_decoded_stream(doc) else {
            continue;
        };
        doc.data.extend(decoded);
    }

    doc.it = start;
    // print_raw(&doc.data, doc.it, 100000);

    let mut text_objects : Vec<Text> = Vec::new();
    let mut text: Text = Text{pos_y : -1.0, chars : String::new(), scaled_font_size : 0.0, font : String::new(), avg_font_size : 1.0};
    let mut text_reader = TextReader{
        y_pos : 0.0, scale : 1.0, font_size : 1.0, scaled_font_size : 1.0, leading : 0.0, graph_scale : 1.0, graph_y : 0.0
    };
    let mut text_reader_stack : Vec<TextReader> = Vec::new();
    text_reader_stack.push(text_reader);

    while doc.it < doc.size() {
        // Find BT section
        parse_page_content(doc, &mut text_reader_stack);

        if doc.it >= doc.size(){
            break;
        }
        
        // BT section starts here
        let Some(top_graph_state) = text_reader_stack.last_mut() else {
            return None;
        };
        parse_text_section(doc, &mut text_objects, &mut text, &fonts, top_graph_state)?;
    }
    add_text_section(&mut text, &mut text_objects, 0.0, 0.0);
    Some(text_objects)
}

/// Parses the Non-text parts of a page
fn parse_page_content(doc : &mut Document, text_reader_stack : &mut Vec<TextReader>){
    let mut stack : Vec<PdfVar> = Vec::new();

    while doc.it < doc.size() {
        match doc.byte() {
            b'B' => {
                if cmp_u8(&doc.data, doc.it, b"BT"){
                    doc.it += 2;
                    return;
                }
            }
            (48..58) | b'+' | b'-' | b'.' => {
                obj_parse_numeric(doc, &mut stack);
                doc.it += 1;
                continue;
            }
            b'q' => {
                if let Some(top) = text_reader_stack.last(){
                    let copy = top.clone();
                    text_reader_stack.push(copy);
                }
            }
            b'Q' => {
                text_reader_stack.pop();
            }
            b'c' => {
                if cmp_u8(&doc.data, doc.it, b"cm"){
                    graph_cm(text_reader_stack, &stack);
                    doc.it += 1;
                }
            }
            _ => {
            }
        }
        doc.it += 1;
        stack.clear();
    }
}

fn graph_cm(text_reader_stack : &mut Vec<TextReader>, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(ty_obj) = stack.get(5) else{
        return None;
    };
    let Some(scale_obj) = stack.get(0) else{
        return None;
    };

    // Get values
    let Some(ty) = ty_obj.get_f64() else{
        return None;
    };
    let Some(new_scale) = scale_obj.get_f64() else{
        return None;
    };

    if let Some(tr) = text_reader_stack.last_mut() {
        tr.graph_scale *= new_scale;
        tr.graph_y += ty;
        return Some(());
    }
    return None;
}

/// Parses a BT section reading all text elements
fn parse_text_section(doc : &mut Document, text_objects : &mut Vec<Text>, text : &mut Text, fonts : &Vec<Font>, tr : &mut TextReader) -> Option<()>{    
    tr.scale = 1.0;
    tr.y_pos = 0.0;
    tr.leading = 0.0;
    tr.scaled_font_size = tr.font_size;
    
    let mut stack : Vec<PdfVar> = Vec::new();

    loop {
        doc.skip_whitespace();
        match doc.byte() {
            b'T' => {
                doc.it += 1;
                match doc.byte() {
                    b'f' => {
                        text_tf(tr,text, &stack)?;
                    }
                    b'J' => {
                        text_tj_array(tr, text, text_objects, fonts, &stack)?;
                    }
                    b'm' => {
                        text_tm(tr, text, &stack)?;
                    }
                    b'd' => {
                        text_td(tr, text, &stack)?;
                    }
                    b'D' => {
                        text_tl(tr, true, &stack)?;
                        text_td(tr, text, &stack)?;
                    }
                    b'*' => {
                        text_asterisk(tr)?;
                    }
                    b'L' => {
                        text_tl(tr, false, &stack)?;
                    }
                    b'j' => {
                        text_tj(tr, text, text_objects, fonts, &stack)?;
                    }
                    b'c' | b'w' | b'z' | b'r' | b's' => {
                        // Ignore
                    }
                    _ => {
                        // Unmatched T
                    }
                }
                doc.it +=1;
                stack.clear();
            }
            b'\'' => {
                text_asterisk(tr);
                text_tj(tr, text, text_objects, fonts, &stack)?;
                stack.clear();
                doc.it += 1;
            }
            b'"' => {
                // Not handled yet
                return None;
            }
            b'E' => {
                if cmp_u8(&doc.data, doc.it, b"ET"){
                    doc.it += 2;
                    break;
                }
                else{
                    read_text(doc);
                    stack.clear();
                }
            }
            _ => {
                if let Err(e) = parse_object(doc, &mut stack){
                    match e {
                        PdfError::UnmatchedChar => {
                            read_text(doc);
                            stack.clear();
                            doc.it += 1;
                        }
                        _ => {
                            return None;
                        }
                    }
                }
            }
        }
    }
    Some(())
}

/// Handles Tj
fn text_tj(tr : &mut TextReader, text : &mut Text, text_objects : &mut Vec<Text>, fonts : &Vec<Font>, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(str_obj) = stack.get(0) else{
        return None;
    };
    
    eval_text_section(text, text_objects, tr.y_pos+tr.graph_y, tr.scaled_font_size);

    // Add text
    let PdfVar::StringLiteral(string_lit) = str_obj else{
        return None;
    };
    add_str_lit(text, tr, string_lit, fonts);
    Some(())
}

/// Handles T*
fn text_asterisk(tr : &mut TextReader) -> Option<()>{
    tr.y_pos += -tr.leading*tr.scale*tr.graph_scale;
    Some(())
}

/// Handles Td
fn text_td(tr : &mut TextReader, text : &mut Text, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(tx_obj) = stack.get(0) else {
        return None;
    };
    let Some(ty_obj) = stack.get(1) else {
        return None;
    };
    let Some(tx) = tx_obj.get_f64() else {
        return None;
    };
    let Some(ty) = ty_obj.get_f64() else {
        return None;
    };

    // If x-move is large, we have a space
    if tx*tr.graph_scale > 160.0{
        text.add_space();
    }

    // Set new value for y-pos, if it is a new BT section position is reset to 0 and then updated
    tr.y_pos = tr.y_pos + ty * tr.scale * tr.graph_scale;
    Some(())
}

/// Handles Tm
fn text_tm(tr : &mut TextReader, text : &mut Text, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(ty_obj) = stack.get(5) else{
        return None;
    };
    let Some(scale_obj) = stack.get(0) else{
        return None;
    };
    let Some(tx_obj) = stack.get(4) else{
        return None;
    };

    // Get values
    let Some(tx) = tx_obj.get_f64() else{
        return None;
    };
    let Some(ty) = ty_obj.get_f64() else{
        return None;
    };
    let Some(new_scale) = scale_obj.get_f64() else{
        return None;
    };

    if tx > 0.0 {
        text.add_space();
    }
    
    tr.scale = new_scale;
    tr.y_pos = ty;
    tr.scaled_font_size = tr.font_size*tr.scale*tr.graph_scale;
    Some(())
}

/// Handles TL
fn text_tl(tr : &mut TextReader, inverse : bool, stack : &Vec<PdfVar>) -> Option<()>{
    let mut index = 0;
    let mut factor = 1.0;
    if inverse {
        index = 1;
        factor = -1.0;
    }
    let Some(l_obj) = stack.get(index) else{
        return None;
    };
    let Some(lf) = l_obj.get_f64() else{
        return None;
    };
    tr.leading = lf*factor;
    Some(())
}

/// Handles Tf
fn text_tf(tr : &mut TextReader, text : &mut Text, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(font_name_obj) = stack.get(0) else{
        return None;
    };
    let Some(font_name) = font_name_obj.get_name() else{
        return None;
    };
    let Some(font_size_obj) = stack.get(1) else {
        return None;
    };
    let Some(font_size_tmp) = font_size_obj.get_f64() else{
        return None;
    };

    text.font = font_name;
    tr.font_size = font_size_tmp;
    tr.scaled_font_size = tr.font_size*tr.scale*tr.graph_scale;
    Some(())
}

/// Handle TJ
fn text_tj_array(tr : &mut TextReader, text : &mut Text, text_objects : &mut Vec<Text>, fonts : &Vec<Font>, stack : &Vec<PdfVar>) -> Option<()>{
    let Some(tj_obj) = stack.get(0) else{
        return None;
    };
    let PdfVar::Array(tj_array) = tj_obj else{
        return None;
    };

    eval_text_section(text, text_objects, tr.y_pos+tr.graph_y, tr.scaled_font_size);

    // Add the text to the text section
    for pdfvar in tj_array{
        if let Some(num) = pdfvar.get_f64(){
            if num < -165.0 {
                text.add_space();
            }
            continue;
        }
        if let PdfVar::StringLiteral(string_lit) = pdfvar {
            add_str_lit(text, tr, string_lit, fonts);
            continue;
        }
        return None;
    }
    Some(())
}

fn add_str_lit(text : &mut Text, tr : &TextReader, string_lit : &Vec<u32>, fonts : &Vec<Font>){
    // Fetch font
    let mut font : &Font = &fonts[0];
    for f in fonts{
        if f.name == text.font{
            font = f;
            break;
        }
    }

    let mut s = String::new();
    let pre_size = text.chars.len() as f64;
    let mut sum = pre_size*text.avg_font_size;

    // Iterate over all chars
    for key in string_lit{
        if *key == 0{
            continue;
        }
        let Some(x_vec) = font.mapping.get(key) else {
            text.chars.push_str(decode_pdfdoc_char(*key).as_str());
            s.push_str(decode_pdfdoc_char(*key).as_str());
            continue;
        };
        for x in x_vec{
            let Some(uc) = char::from_u32(*x) else {
                continue;
            };
            s.push(uc);
            text.chars.push(uc);
        }
    }
    // !("s {}, {}",s, text.pos_y);
    // Update average font size
    if text.chars.len() > 0 {
        let post_size = text.chars.len() as f64;
        sum += (post_size-pre_size)*tr.scaled_font_size;
        text.avg_font_size = sum/post_size;
    }
}

/// Evaluates if a new text segment belongs to the current text section, creates a new text section otherwise
fn eval_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // Compare y-position of last text to the new one
    let diff = (text.pos_y-y_pos).abs();

    if diff > 2.0*text.scaled_font_size {
        // New Text section
        add_text_section(text, text_objects, y_pos, scaled_font_size);
    } else if diff > 0.7*scaled_font_size {
        // New row, look if fontsize has changed
        if (text.scaled_font_size-scaled_font_size).abs() > 0.2{
            add_text_section(text, text_objects, y_pos, scaled_font_size);
        }
        else{
            // Update the y-value of the text segment
            text.add_space();
            text.pos_y = y_pos;
        }
    }
    text.scaled_font_size = scaled_font_size;
}

/// Saves the previous text section, creates a new text section to write to
fn add_text_section(text : &mut Text, text_objects : &mut Vec<Text>, y_pos : f64, scaled_font_size : f64){
    // New text section
    if text.chars.len() > 0{
        // Save previous text segment when new is found
        let mut copy = text.clone();
        copy.font = String::new();
        text_objects.push(copy);
        text.chars.clear();
    }
    text.pos_y = y_pos;
    text.scaled_font_size = scaled_font_size;
    text.avg_font_size = 0.0;
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

