use std::collections::HashMap;

use crate::{decoding::{decode_flate, decode_pdfdoc, handle_decodeparms}, document::{Document, PdfDocError, Reader}, print_raw};

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

#[derive(Debug)]
pub enum PdfParseError {
    NumParseError,
    EmptyDictionary,
    DictionaryKeyNotName,
    IncorrectHexNumber,
    IncorrectHexString,
    NoStringEnding,
    NameCharNotAllowed,
    UnknownConst,
    ObjectRefError,
    ObjectRefNotInteger,
    ObjectRefSignedInteger,
    UnmatchedChar,
    NoEndobjFound,
    EmptyObject,
    NoObjRef,
    PdfLoadError,
    PdfHeaderError,
    StartXrefNotFound,
    OutOfBounds,
    XrefObjectError,
    StreamError,
    FlateDecodeError,
    DecodeDataError,
    UnknownFilter,
    XrefTableError,
    TrailerNotFound,
    TrailerError,
    XrefError,
}

impl PdfVar {
    /// Returns a value from an object's dictionary. If that object contains a dictionary
    /// Or directly from a dictionary object
    pub fn get_dict_value(&self, key : &str) -> Option<&PdfVar>{
        // Case 1: The object is a dictionary
        if let PdfVar::Dictionary(dict) = self{
            return dict.get(key);
        }
        
        // Case 2: The object is an object, and it contains a dictionary
        let PdfVar::Object{ _id, content } = &self else{
            return None;
        };
        
        let Some(dict_obj) = content.get(1) else{
            return None;
        };
        if let PdfVar::Dictionary(dict) = dict_obj{
            return dict.get(key);
        }
        return None;
    }

    /// Get an array from unsigned integer array, or from an integer
    pub fn get_usize_array(&self) -> Option<Vec<usize>>{
        let PdfVar::Array(array) = self else{
            if let Some(value) = self.get_usize(){
                return Some(vec![value]);
            }
            return None;
        };
        let mut output : Vec<usize> = Vec::new();
        for obj in array{
            let value = obj.get_usize()?;
            output.push(value);
        }
        return Some(output);
    }

    /// Get integer value from dictionary key
    pub fn get_dict_int(&self, key : &str) -> Option<usize> {
        let int_object = self.get_dict_value(key)?;
        int_object.get_usize()
    }

    /// Returns the String of a name object
    pub fn get_name(&self) -> Option<String>{
        if let PdfVar::Name(name) = self{
            return Some(name.to_string());
        };
        return None;
    }

    pub fn get_str(&self) -> Option<String>{
        if let PdfVar::StringLiteral(array) = self {
            return Some(decode_pdfdoc(array));
        };
        return None;
    }

    /// Get usize from unsigned integer,or indirect object
    pub fn get_usize(&self) -> Option<usize>{
        if let PdfVar::Integer { value, signed } = self {
            if *signed{
                return None;
            }
            return Some(*value as usize);
        };
        if let PdfVar::IndirectObject(value) = self {
            return Some(*value);
        }
        return None;
    }

    /// Get f64 from integer or real
    pub fn get_f64(&self) -> Option<f64>{
        if let PdfVar::Integer { value, signed } = self {
            return Some(*value as f64);
        };
        if let PdfVar::Real(f) = self {
            return Some(*f);
        };
        return None;
    }

    /// Returns a decoded stream
    pub fn get_decoded_stream(&self, rd : &mut Reader) -> Option<Vec<u8>>{
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
        let mut filter = String::new();
        if let Some(filter_obj) = self.get_dict_value("Filter") {
            if let Some(fname) = filter_obj.get_name(){
                filter = fname;
            }
        };

        // Match filter type
        match filter.as_str() {
            "FlateDecode" =>{
                return decode_flate_stream(rd, *start, *size, self);
            }
            "" => {
                println!("Empty filter");
            }
            _ => {
                println!("Unknown filter type {}", filter);
            }
        }
        
        return None;
    }

    /// Returns stream (start,size)
    pub fn get_stream(&self) -> Option<(usize,usize)>{
        let PdfVar::Object{ _id, content } = &self else{
            return None;
        };
        
        let Some(object_stream) = content.get(2) else{
            return None;
        };

        let PdfVar::Stream{start : stream_start, size:stream_size} = object_stream else{
            return None;
        };
        return Some((*stream_start, *stream_size));
    }

    /// Parses a document object starting from index
    pub fn from(rd : &mut Reader, index : usize) -> Result<Self, PdfParseError>{
        // The objects inside this object is placed into obj_stack
        let mut obj_stack : Vec<PdfVar> = Vec::new();
        rd.it = index;
        rd.skip_whitespace();
        
        // Loop until endobj-tag is found
        loop {
            if rd.it >= rd.size(){
                return Err(PdfParseError::NoEndobjFound);
            }
            parse_object(rd, &mut obj_stack)?;
            rd.skip_whitespace();
            
            if cmp_u8(&rd.data, rd.it, b"endobj"){
                break;
            }
        }

        if obj_stack.len() < 2{
            return Err(PdfParseError::EmptyObject);
        }
        let Some(first_obj) = obj_stack.get(0) else{
            return Err(PdfParseError::EmptyObject);
        };
        let PdfVar::ObjectRef(obj_ref) = first_obj else{
            return Err(PdfParseError::NoObjRef);
        };
        return Ok(PdfVar::Object{_id:obj_ref.clone(), content:obj_stack});
    }
}

// Help functions:

/// Parses pdf types (int, real, array, indirect obj, strings, dictionaries, names, booleans, null, stream)
pub fn parse_object(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(),PdfParseError>{
    rd.skip_whitespace();
    match rd.byte(){
        (48..58) | b'+' | b'-' | b'.' =>{
            // First char is numeric or +/-, can be float, int, (indirect obj)
            return obj_parse_numeric(rd, stack);
        }
        b'[' => {
            // First char [, array
            return obj_parse_array(rd, stack);
        }
        b'<' => {
            // Hexstring or dictionary, depending on next char
            if rd.data[rd.it+1] == b'<' {
                // Dictionary
                return obj_parse_dictionary(rd, stack);
            }
            else{
                // Hexstring
                return obj_parse_hex_string(rd, stack);
            }
        }
        b'(' => {
            // String literal
            return obj_parse_string_literal(rd, stack);
        }
        b'/' => {
            // Name
            return obj_parse_name(rd, stack);
        }
        b'n' | b't' | b'f' => {
            // null, true, false
            return obj_parse_const(rd, stack);
        }
        b'R' | b'o' => {
            // End of indirect object, or start of object
            return obj_parse_object_ref(rd, stack);
        }
        b's' =>{
            // Stream
            return obj_parse_stream(rd, stack);
        }
        _ => {
            // println!("uk char {}", rd.byte() as char);
            // print_raw(&rd.data, rd.it, 40);
            // println!("\n----");
        }
    }
    Err(PdfParseError::UnmatchedChar)
}

/// Decodes a FlateDecode stream
fn decode_flate_stream(rd : &mut Reader, start : usize, size : usize, obj_dict : &PdfVar) -> Option<Vec<u8>>{
    let Some(decoded) = decode_flate(rd, start, size) else {
        return None;
    };

    if let Some(decodeparms_obj) = obj_dict.get_dict_value("DecodeParms"){
        return handle_decodeparms(decoded, decodeparms_obj);
    };
    return Some(decoded);
}

/// Parse array object
fn obj_parse_array(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    rd.it += 1;

    // The objects in the array are stored in the array stack
    let mut array_stack : Vec<PdfVar> = Vec::new();
    loop {
        rd.skip_whitespace();
        if rd.byte() == b']'{
            rd.it += 1;
            break;
        }
        parse_object(rd, &mut array_stack)?;
    }

    stack.push(PdfVar::Array(array_stack));
    Ok(())
}

/// Parse null, true and false
fn obj_parse_const(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    if cmp_u8(&rd.data, rd.it, b"null"){
        rd.it += 4;
        stack.push(PdfVar::Null);
    } else if cmp_u8(&rd.data, rd.it, b"true") {
        rd.it += 4;
        stack.push(PdfVar::Boolean(true));
    } else if cmp_u8(&rd.data, rd.it, b"false") {
        rd.it += 5;
        stack.push(PdfVar::Boolean(false));
    } else{
        return Err(PdfParseError::UnmatchedChar);
    }
    Ok(())
}

/// Parse dictionary object
fn obj_parse_dictionary(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    // Skip <<
    rd.it += 2;
    let mut dict_stack : Vec<PdfVar> = Vec::new();

    loop {
        rd.skip_whitespace();
        if cmp_u8(&rd.data, rd.it, b">>"){
            rd.it += 2;
            break;
        }
        parse_object(rd, &mut dict_stack)?;
    }
    
    // Convert list to hashmap
    let mut dict : HashMap<String, PdfVar> = HashMap::new();

    // Pop 2 items at a time, key and value
    while dict_stack.len() > 0{
        let Some(obj2) = dict_stack.pop() else{
            return Err(PdfParseError::EmptyDictionary); 
        };
        let Some(obj1) = dict_stack.pop() else {
            return Err(PdfParseError::EmptyDictionary); 
        };
        let PdfVar::Name(obj1_name) = obj1 else{
            return Err(PdfParseError::DictionaryKeyNotName);
        };
        dict.insert(obj1_name, obj2);
    }
    stack.push(PdfVar::Dictionary(dict));
    Ok(())
}

/// Parse Hex String
fn obj_parse_hex_string(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    let mut hex_vector : Vec<u32> = Vec::new();
    rd.it += 1;

    while rd.byte().is_ascii_alphanumeric() {
        let c1 = rd.byte();
        let mut c2 = rd.data[rd.it+1];
        
        // If last char is not included, it is assumed to be 0
        if !c2.is_ascii_alphanumeric(){
            c2 = b'0';
            rd.it -= 1;
        }

        let number = to_hex(c1, c2)?;
        hex_vector.push(number as u32);
        rd.it += 2;
    }
    if rd.byte() != b'>' {
        return Err(PdfParseError::IncorrectHexString);
    }
    rd.it += 1;
    stack.push(PdfVar::StringLiteral(hex_vector));
    Ok(())
}

/// Parse pdf name object
fn obj_parse_name(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    let mut chars : Vec<u32> = Vec::new();
    rd.it += 1;

    loop {
        if rd.byte() == b'#'{
            // Convert to hex
            let hex = to_hex(rd.data[rd.it+1], rd.data[rd.it+2])?;
            chars.push(hex as u32);
            rd.it += 3;
        } else if is_delimiter(&rd.data, rd.it){
            break;
        } else if (31..127).contains(&rd.byte()){
            chars.push(rd.byte() as u32);
            rd.it += 1;
        } else{
            return Err(PdfParseError::NameCharNotAllowed);
        }
    }
    let name : String = decode_pdfdoc(&chars);
    stack.push(PdfVar::Name(name));
    Ok(())
}

/// Parses a numeric object
fn obj_parse_numeric(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    let signed = rd.byte() == b'+' || rd.byte() == b'-';
    let mut number_str = String::new();
    
    while rd.byte().is_ascii_digit() || matches!(rd.data[rd.it], b'+' | b'-' | b'.') {
        number_str.push(rd.byte() as char);
        rd.it += 1;
    }
    
    // Attempts to convert to int, if fails try float, if fail return None
    if let Ok(number_i64) = number_str.parse::<i64>(){
        stack.push(PdfVar::Integer{value : number_i64 as i64, signed : signed});
    } else{
        let Ok(number_f64) = number_str.parse::<f64>() else {
            return Err(PdfParseError::NumParseError);
        };
        stack.push(PdfVar::Real(number_f64));
    }
    Ok(())
}

/// Parse indirec object (D D R) or object head (D D obj)
fn obj_parse_object_ref(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    let mut indirect_obj = false;
    // print_raw(&rd.data, rd.it, 10);
    if rd.byte() == b'R' {
        // Next char has to be a delimiter
        if !is_delimiter(&rd.data, rd.it+1){
            return  Err(PdfParseError::UnmatchedChar);
        }
        rd.it += 1;
        indirect_obj = true;
    } else if cmp_u8(&rd.data, rd.it, b"obj"){
        rd.it += 3;
    } else{
        return Err(PdfParseError::ObjectRefError);
    }
    
    // 2 previous integers gets popped from stack
    let Some(arg_2) = stack.pop() else{
        return Err(PdfParseError::ObjectRefError);
    };
    let Some(arg_1) = stack.pop() else{
        return Err(PdfParseError::ObjectRefError);
    };

    // Inspect arg_1, arg_2
    let PdfVar::Integer{ value : a1_value, signed : a1_signed} = arg_1 else {
        return Err(PdfParseError::ObjectRefNotInteger);
    };
    let PdfVar::Integer{ value : _a2_value, signed : a2_signed} = arg_2 else {
        return Err(PdfParseError::ObjectRefNotInteger);
    };

    if a1_signed || a2_signed{
        return Err(PdfParseError::ObjectRefSignedInteger);
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
fn obj_parse_stream(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    if !cmp_u8(&rd.data, rd.it, b"stream"){
        return Err(PdfParseError::UnmatchedChar);
    }
    rd.it += 6;
    rd.skip_whitespace();
    let mut start:usize = rd.it;
    let Some(stream_dict_obj) = stack.last() else{
        return Err(PdfParseError::StreamError);
    };
    let Some(length_obj) = stream_dict_obj.get_dict_value("Length") else{
        return Err(PdfParseError::StreamError);
    };
    let Some(size) = length_obj.get_usize() else{
        return Err(PdfParseError::StreamError);
    };

    rd.it += size;
    rd.skip_whitespace();
    if !cmp_u8(&rd.data, rd.it, b"endstream"){
        return Err(PdfParseError::StreamError);
    }
    rd.it += 9;
    stack.push(PdfVar::Stream{start:start,size:size});
    Ok(())
}

/// Parse an object string literal
fn obj_parse_string_literal(rd : &mut Reader, stack : &mut Vec<PdfVar>) -> Result<(), PdfParseError>{
    let mut parenthesis_depth = 1;
    let mut literal : Vec<u32> = Vec::new();
    let mut reading_err = false; // set to true when reading error occurs
    loop {
        rd.it += 1;
        if rd.it >= rd.size(){
            return Err(PdfParseError::NoStringEnding);
        }
        match rd.byte() {
            b'\\' => {
                rd.it += 1;
                if reading_err {
                    continue;
                }
                if let Some(()) = handle_escape(rd, &mut literal){} 
                else {
                    reading_err = true;
                }
            }
            b'(' => {
                parenthesis_depth += 1;
                if !reading_err {
                    literal.push(rd.byte() as u32);
                }
            }
            b')' => {
                parenthesis_depth -= 1;
                if parenthesis_depth == 0{
                    break;
                }
                if !reading_err {
                    literal.push(rd.byte() as u32);
                }
            }
            _ => {
                if !reading_err {
                    literal.push(rd.byte() as u32);
                }
            }
        }
    }
    rd.it += 1;
    stack.push(PdfVar::StringLiteral(literal));
    Ok(())
}

/// Converts two hexadecimal symbols into one decimal value
fn to_hex(num1 : u8, num2 : u8) -> Result<u8,PdfParseError>{
    let value1 = match num1 {
        b'A'..=b'F' => num1 - 55,
        b'a'..=b'f' => num1 - 87,
        b'0'..=b'9' => num1 - 48,
        _ => {
            return Err(PdfParseError::IncorrectHexNumber);
        }
    };
    
    let value2 = match num2 {
        b'A'..=b'F' => num2 - 55,
        b'a'..=b'f' => num2 - 87,
        b'0'..=b'9' => num2 - 48,
        _ => {
            return Err(PdfParseError::IncorrectHexNumber);
        },
    };

    Ok(value1*16+value2)
}

/// Parse escape \\ chars
fn handle_escape(rd : &mut Reader, literal : &mut Vec<u32>) -> Option<()>{
    match rd.data[rd.it] {
        b'n' => {
            literal.push(32);
        }
        b'r' | b'b' => {
            // We dont care
        }
        b't' => {
            literal.push(9);
        }
        b'(' | b')' | b'\\' => {
            literal.push(rd.data[rd.it] as u32);
        }
        13 | 10 => {
            // Skip new line chars
            while matches!(rd.data[rd.it], 10 | 13) {
                rd.it += 1;
            }
            rd.it -= 1;
        }
        _ => {
            let mut num : u32 = 0;
            let mut numsize = 0;
            for i in 0..3{
                if (48..56).contains(&rd.data[rd.it+i]){
                    num = num*8+(rd.data[rd.it+i] - 48) as u32;
                    numsize += 1;
                    continue;
                }
                break;
            }
            if numsize == 0{
                return None;
            }
            literal.push(num);
            rd.it += numsize-1;
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
