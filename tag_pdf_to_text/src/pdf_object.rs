use std::{collections::HashMap, vec};

use crate::document::{Document, PdfError};
use crate::decoding::{decode_flate, decode_pdfdoc, handle_decodeparms};

#[derive(Debug, Clone)]
pub enum PdfVar {
    Boolean(bool),
    Integer{value : i64, signed : bool}, // (Value, +/- in front)
    Real(f64),
    StringLiteral(Vec<u32>),
    Name(String),
    Array(Vec<PdfVar>),
    Dictionary(std::collections::HashMap<String, PdfVar>),
    Null,
    Stream{start : usize, size : usize},
    IndirectObject(usize),
    Object{_id : usize, content : Vec<PdfVar>},
    ObjectRef(usize),
}

impl PdfVar {
    /// Returns a value from an object's dictionary. If that object contains a dictionary
    /// Or directly from a dictionary object, or if it's an indirect object pointing to an object containing a dictionary.
    pub fn get_dict_value(&self, key : &str) -> Option<&PdfVar>{
        // Case 1: The object is a dictionary
        if let PdfVar::Dictionary(dict) = self{
            return dict.get(key);
        }
        
        // Case 2: The object is an object, and it contains a dictionary
        if let PdfVar::Object{ _id, content } = &self{
            let Some(dict_obj) = content.get(1) else{
                return None;
            };
            if let PdfVar::Dictionary(dict) = dict_obj{
                return dict.get(key);
            }
            return None;
        };
        return None;
    }
    
    /// Get an array from unsigned integer array, or from an integer
    pub fn get_usize_array(&self) -> Option<Vec<usize>>{
        let PdfVar::Array(array) = self else{
            if let Some(value) = self.get_indirect_obj_index(){
                return Some(vec![value]);
            }
            return None;
        };
        let mut output : Vec<usize> = Vec::new();
        for obj in array{
            let value = obj.get_indirect_obj_index()?;
            output.push(value);
        }
        return Some(output);
    }

    /// Get integer value from dictionary key
    pub fn get_dict_int(&self, key : &str) -> Option<usize> {
        let int_object = self.get_dict_value(key)?;
        int_object.get_indirect_obj_index()
    }

    /// Returns the String of a name object
    pub fn get_name(&self) -> Option<String>{
        if let PdfVar::Name(name) = self{
            return Some(name.to_string());
        };
        return None;
    }

    /// Returns a list of String of name objects in an array. 
    /// If only one Name, an array with one String will be returned
    pub fn get_names(&self) -> Option<Vec<String>> {
        if let PdfVar::Name(name) = self{
            return Some(vec![name.to_string()]);
        };
        let mut names : Vec<String> = Vec::new();
        if let PdfVar::Array(name_arr) = self {
            for elem in name_arr {
                let PdfVar::Name(name) = elem else {
                    return None;
                };
                names.push(name.clone());
            }
            return Some(names);
        }
        return None;
    }

    pub fn get_str(&self) -> Option<String>{
        if let PdfVar::StringLiteral(array) = self {
            return Some(decode_pdfdoc(array));
        };
        return None;
    }

    /// Get index of indirect object. If called with an integer, that value is returned
    pub fn get_indirect_obj_index(&self) -> Option<usize>{
        if let PdfVar::IndirectObject(value) = self{
            return Some(*value);
        };
        if let PdfVar::Integer { value, signed: _ } = self {
            return Some(*value as usize);
        };
        return None;
    }

    /// Get usize from unsigned integer, or indirect object (goes to the indirect object)
    pub fn get_usize(&self, doc : &mut Document) -> Option<usize>{
        if let PdfVar::Integer { value, signed } = self {
            if *signed{
                return None;
            }
            return Some(*value as usize);
        };
        if let PdfVar::IndirectObject(obj_id) = self {
            let doc_ix = doc.it;
            let Some(object) = doc.get_object_by_id(*obj_id) else {
                doc.it = doc_ix;
                return None;
            };
            doc.it = doc_ix;
            return object.get_usize(doc);
        }
        if let PdfVar::Object { _id, content } = self {
            let Some(integer) = content.get(1) else {
                return None;
            };
            if let PdfVar::Integer { value, signed: _ } = integer{
                return Some(value.abs() as usize);
            }
        }
        return None;
    }

    /// Get f64 from integer or real
    pub fn get_f64(&self) -> Option<f64>{
        if let PdfVar::Integer { value, signed: _ } = self {
            return Some(*value as f64);
        };
        if let PdfVar::Real(f) = self {
            return Some(*f);
        };
        return None;
    }

    /// Returns a decoded stream
    pub fn get_decoded_stream(&self, doc : &mut Document) -> Option<Vec<u8>>{
        // Self must be an object
        let PdfVar::Object { _id, content } = &self else{
            return None;
        };
        
        // Stream object should be on index 2
        let Some(stream_obj) = content.get(2) else{
            return None;
        };
        
        // Fetch stream information
        let PdfVar::Stream { start, size } = stream_obj else{
            return None;
        };

        // Get filter type
        let mut filters : Vec<String> = Vec::new();
        if let Some(filter_obj) = self.get_dict_value("Filter") {
            if let Some(fname) = filter_obj.get_names(){
                filters = fname;
            }
        };

        // Match filter type
        let mut decoded = doc.data[*start..*start+*size].to_vec();
        for filter in filters{
            match filter.as_str() {
                "FlateDecode" =>{
                    let Ok(_) = decode_flate(&mut decoded) else {
                        return None;
                    };
                }
                "LZWDecode" => {
                    // Not implemented
                    return None;
                }
                "" => {
                    decoded = doc.data[*start..*start+*size].to_vec();
                }
                _ => {
                    // Unknown filter type
                    return None;
                }
            }
        }

        // Handle DecodeParms
        if let Some(decodeparms_obj) = self.get_dict_value("DecodeParms"){
            if let Ok(vec) = handle_decodeparms(decoded, decodeparms_obj, doc) {
                return Some(vec);
            }
            return None;
        };
        
        return Some(decoded);
    }

    /// Parses a document object starting from index
    pub fn from(doc : &mut Document, index : usize) -> Result<Self, PdfError>{
        // The objects inside this object is placed into obj_stack
        let mut obj_stack : Vec<PdfVar> = Vec::new();
        doc.it = index;
        doc.skip_whitespace();
        
        // Loop until endobj-tag is found
        loop {
            if doc.it >= doc.size(){
                return Err(PdfError::ObjectError);
            }
            parse_object(doc, &mut obj_stack)?;
            doc.skip_whitespace();
            
            if cmp_u8(&doc.data, doc.it, b"endobj"){
                break;
            }
        }

        if obj_stack.len() < 2{
            return Err(PdfError::ObjectError);
        }
        let Some(first_obj) = obj_stack.get(0) else{
            return Err(PdfError::ObjectError);
        };
        let PdfVar::ObjectRef(obj_ref) = first_obj else{
            return Err(PdfError::ObjectError);
        };
        return Ok(PdfVar::Object{_id:obj_ref.clone(), content:obj_stack});
    }
}

// Help functions:

/// Parses pdf types (int, real, array, indirect obj, strings, dictionaries, names, booleans, null, stream)
pub fn parse_object(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(),PdfError>{
    doc.skip_whitespace();
    match doc.byte(){
        (48..58) | b'+' | b'-' | b'.' =>{
            // First char is numeric or +/-, can be float, int, (indirect obj)
            return obj_parse_numeric(doc, stack);
        }
        b'[' => {
            // First char [, array
            return obj_parse_array(doc, stack);
        }
        b'<' => {
            // Hexstring or dictionary, depending on next char
            if doc.data[doc.it+1] == b'<' {
                // Dictionary
                return obj_parse_dictionary(doc, stack);
            }
            else{
                // Hexstring
                return obj_parse_hex_string(doc, stack);
            }
        }
        b'(' => {
            // String literal
            return obj_parse_string_literal(doc, stack);
        }
        b'/' => {
            // Name
            return obj_parse_name(doc, stack);
        }
        b'n' | b't' | b'f' => {
            // null, true, false
            return obj_parse_const(doc, stack);
        }
        b'R' | b'o' => {
            // End of indirect object, or start of object
            return obj_parse_object_ref(doc, stack);
        }
        b's' =>{
            // Stream
            return obj_parse_stream(doc, stack);
        }
        _ => {
        }
    }
    Err(PdfError::UnmatchedChar)
}

/// Parse array object
fn obj_parse_array(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    doc.it += 1;

    // The objects in the array are stored in the array stack
    let mut array_stack : Vec<PdfVar> = Vec::new();
    loop {
        doc.skip_whitespace();
        if doc.byte() == b']'{
            doc.it += 1;
            break;
        }
        parse_object(doc, &mut array_stack)?;
    }

    stack.push(PdfVar::Array(array_stack));
    Ok(())
}

/// Parse null, true and false
fn obj_parse_const(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    if cmp_u8(&doc.data, doc.it, b"null"){
        doc.it += 4;
        stack.push(PdfVar::Null);
    } else if cmp_u8(&doc.data, doc.it, b"true") {
        doc.it += 4;
        stack.push(PdfVar::Boolean(true));
    } else if cmp_u8(&doc.data, doc.it, b"false") {
        doc.it += 5;
        stack.push(PdfVar::Boolean(false));
    } else{
        return Err(PdfError::UnmatchedChar);
    }
    Ok(())
}

/// Parse dictionary object
fn obj_parse_dictionary(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    // Skip <<
    doc.it += 2;
    let mut dict_stack : Vec<PdfVar> = Vec::new();

    loop {
        doc.skip_whitespace();
        if cmp_u8(&doc.data, doc.it, b">>"){
            doc.it += 2;
            break;
        }
        parse_object(doc, &mut dict_stack)?;
    }
    
    // Convert list to hashmap
    let mut dict : HashMap<String, PdfVar> = HashMap::new();

    // Pop 2 items at a time, key and value
    while dict_stack.len() > 0{
        let Some(obj2) = dict_stack.pop() else{
            return Err(PdfError::DictionaryError); 
        };
        let Some(obj1) = dict_stack.pop() else {
            return Err(PdfError::DictionaryError); 
        };
        let PdfVar::Name(obj1_name) = obj1 else{
            return Err(PdfError::DictionaryError);
        };
        dict.insert(obj1_name, obj2);
    }
    stack.push(PdfVar::Dictionary(dict));
    Ok(())
}

/// Parse Hex String
fn obj_parse_hex_string(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    let mut hex_vector : Vec<u32> = Vec::new();
    doc.it += 1;

    while doc.byte().is_ascii_alphanumeric() {
        let mut chars : Vec<u8> = vec![doc.byte(), doc.data[doc.it+1]];

        // If last char is not included, it is assumed to be 0
        if !chars[1].is_ascii_alphanumeric(){
            chars[1] = b'0';
            doc.it -= 1;
        }

        let number = to_hex(&chars)?;
        hex_vector.push(number as u32);
        doc.it += 2;
    }
    if doc.byte() != b'>' {
        return Err(PdfError::HexError);
    }
    doc.it += 1;
    stack.push(PdfVar::StringLiteral(hex_vector));
    Ok(())
}

/// Parse pdf name object
fn obj_parse_name(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    let mut chars : Vec<u32> = Vec::new();
    doc.it += 1;

    loop {
        if doc.byte() == b'#'{
            // Convert to hex
            let nums = vec![doc.data[doc.it+1], doc.data[doc.it+2]];
            let hex = to_hex(&nums)?;
            chars.push(hex as u32);
            doc.it += 3;
        } else if is_delimiter(&doc.data, doc.it){
            break;
        } else if (31..127).contains(&doc.byte()){
            chars.push(doc.byte() as u32);
            doc.it += 1;
        } else{
            return Err(PdfError::ObjectError);
        }
    }
    let name : String = decode_pdfdoc(&chars);
    stack.push(PdfVar::Name(name));
    Ok(())
}

/// Parses a numeric object
fn obj_parse_numeric(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    let signed = doc.byte() == b'+' || doc.byte() == b'-';
    let mut number_str = String::new();
    
    while doc.byte().is_ascii_digit() || matches!(doc.data[doc.it], b'+' | b'-' | b'.') {
        number_str.push(doc.byte() as char);
        doc.it += 1;
    }
    
    // Attempts to convert to int, if fails try float, if fail return None
    if let Ok(number_i64) = number_str.parse::<i64>(){
        stack.push(PdfVar::Integer{value : number_i64 as i64, signed : signed});
    } else{
        let Ok(number_f64) = number_str.parse::<f64>() else {
            return Err(PdfError::ObjectError);
        };
        stack.push(PdfVar::Real(number_f64));
    }
    Ok(())
}

/// Parse indirec object (D D R) or object head (D D obj)
fn obj_parse_object_ref(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    let mut indirect_obj = false;
    if doc.byte() == b'R' {
        // Next char has to be a delimiter
        if !is_delimiter(&doc.data, doc.it+1){
            return Err(PdfError::UnmatchedChar);
        }
        doc.it += 1;
        indirect_obj = true;
    } else if cmp_u8(&doc.data, doc.it, b"obj"){
        doc.it += 3;
    } else{
        return Err(PdfError::ObjectRefError);
    }
    
    // 2 previous integers gets popped from stack
    let Some(arg_2) = stack.pop() else{
        return Err(PdfError::ObjectRefError);
    };
    let Some(arg_1) = stack.pop() else{
        return Err(PdfError::ObjectRefError);
    };

    // Inspect arg_1, arg_2
    let PdfVar::Integer{ value : a1_value, signed : a1_signed} = arg_1 else {
        return Err(PdfError::ObjectRefError);
    };
    let PdfVar::Integer{ value : _a2_value, signed : a2_signed} = arg_2 else {
        return Err(PdfError::ObjectRefError);
    };

    if a1_signed || a2_signed{
        return Err(PdfError::ObjectRefError);
    }

    if indirect_obj {
        stack.push(PdfVar::IndirectObject(a1_value as usize));
    }
    else {
        stack.push(PdfVar::ObjectRef(a1_value as usize));
    }
    Ok(())
}

/// Parse an object stream
fn obj_parse_stream(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    if !cmp_u8(&doc.data, doc.it, b"stream"){
        return Err(PdfError::UnmatchedChar);
    }

    doc.it += 6;
    doc.skip_whitespace();
    
    let start:usize = doc.it;
    let Some(stream_dict_obj) = stack.last() else{
        return Err(PdfError::StreamError);
    };
    let Some(length_obj) = stream_dict_obj.get_dict_value("Length") else{
        return Err(PdfError::StreamError);
    };
    let Some(size) = length_obj.get_usize(doc) else{
        return Err(PdfError::StreamError);
    };

    doc.it += size;
    doc.skip_whitespace();

    if !cmp_u8(&doc.data, doc.it, b"endstream"){
        return Err(PdfError::StreamError);
    }
    doc.it += 9;
    stack.push(PdfVar::Stream{start:start,size:size});
    Ok(())
}

/// Parse an object string literal
fn obj_parse_string_literal(doc : &mut Document, stack : &mut Vec<PdfVar>) -> Result<(), PdfError>{
    let mut parenthesis_depth = 1;
    let mut literal : Vec<u32> = Vec::new();
    let mut reading_err = false; // set to true when reading error occurs
    loop {
        doc.it += 1;
        if doc.it >= doc.size(){
            return Err(PdfError::ObjectError);
        }
        match doc.byte() {
            b'\\' => {
                doc.it += 1;
                if reading_err {
                    continue;
                }
                if let Some(()) = handle_escape(doc, &mut literal){} 
                else {
                    reading_err = true;
                }
            }
            b'(' => {
                parenthesis_depth += 1;
                if !reading_err {
                    literal.push(doc.byte() as u32);
                }
            }
            b')' => {
                parenthesis_depth -= 1;
                if parenthesis_depth == 0{
                    break;
                }
                if !reading_err {
                    literal.push(doc.byte() as u32);
                }
            }
            _ => {
                if !reading_err {
                    literal.push(doc.byte() as u32);
                }
            }
        }
    }
    doc.it += 1;
    stack.push(PdfVar::StringLiteral(literal));
    Ok(())
}

/// Converts hexadecimal symbols into one decimal value
pub fn to_hex(nums : &[u8]) -> Result<u32,PdfError>{
    let mut value : u32 = 0;
    for num in nums{
        let v1 = match num {
            b'A'..=b'F' => num - 55,
            b'a'..=b'f' => num - 87,
            b'0'..=b'9' => num - 48,
            _ => {
                return Err(PdfError::HexError);
            }
        };
        value = value * 16 + v1 as u32;
    }

    Ok(value)
}

/// Parse escape \\ chars
fn handle_escape(doc : &mut Document, literal : &mut Vec<u32>) -> Option<()>{
    match doc.data[doc.it] {
        b'n' => {
            literal.push(32);
        }
        b'r' => {
            // We dont care, or do we
            literal.push(13);
        }
        b'b' => {
            literal.push(8);
        }
        b't' => {
            literal.push(9);
        }
        b'(' | b')' | b'\\' => {
            literal.push(doc.data[doc.it] as u32);
        }
        13 | 10 => {
            // Skip new line chars
            while matches!(doc.data[doc.it], 10 | 13) {
                doc.it += 1;
            }
            doc.it -= 1;
        }
        _ => {
            let mut num : u32 = 0;
            let mut numsize = 0;
            for i in 0..3{
                if (48..56).contains(&doc.data[doc.it+i]){
                    num = num*8+(doc.data[doc.it+i] - 48) as u32;
                    numsize += 1;
                    continue;
                }
                break;
            }
            if numsize == 0{
                return None;
            }
            literal.push(num);
            doc.it += numsize-1;
        }
    }
    Some(())
}

/// Compares a byte array to a slice of a vector, returns true if they match
pub fn cmp_u8(vector : &Vec<u8>, index : usize, byte_array : &[u8]) -> bool{
    for i in 0..byte_array.len(){
        if i+index >= vector.len(){
            return false;
        }
        if vector[i+index] != byte_array[i]{
            return false;
        }
    }
    return true;
}

/// Determines if a char in a vector on index is a delimiter
pub fn is_delimiter(vector : &Vec<u8>, index: usize) -> bool{
    matches!(vector[index],0|10|12|13|32|40|41|60|62|91|93|123|125|47|37)
}
