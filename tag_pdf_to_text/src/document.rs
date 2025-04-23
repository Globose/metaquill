use std::{fs::{self}, io};

use crate::text_parser::{get_page_resources, read_objects_text, Text};
use crate::pdf_object::{cmp_u8, parse_object, PdfVar};
use crate::decoding::get_256_repr;

#[derive(Debug)]
struct Trailer{
    info : usize, // optional, documents information dictionary
    root : usize, // Catalog directory
    size : usize, // total number of entries in the files xref-tables
    encrypt : usize,
    // Prev, id are not stored
}

#[derive(Debug)]
struct ObjectRef {
    compressed : u8,
    xref : usize,
    _version : usize,
}

#[derive(Debug)]
pub struct Document {
    xref : Vec<ObjectRef>,
    trailer : Trailer,
    pub(crate) data : Vec<u8>,
    pub(crate) it : usize,
}

#[derive(Debug)]
pub enum PdfError {
    DecodeError,
    DictionaryError,
    DocumentError,
    HexError,
    LoadError,
    ObjectError,
    ObjectRefError,
    PdfHeaderError,
    StreamError,
    UnmatchedChar,
    XrefError,
}

// Constants
const U_STARTXREF : &[u8] = b"startxref";
const U_PDF : &[u8] = b"%PDF";
const U_TRAILER : &[u8] = b"trailer";
const U_XREF : &[u8] = b"xref";
const C_PREV : &str = "Prev";
const C_SIZE : &str = "Size";

impl Document {
    /// Read Text Sections From Page
    pub fn get_text_from_page(&mut self, page_nr : usize) -> Option<Vec<Text>>{
        let Some(page_object) = self.get_page_no(page_nr) else{
            return None;
        };
        
        // Get Page Fonts
        let fonts = get_page_resources(self, &page_object);

        // Get ids of content
        // If contents is non-existent page is empty
        let Some(page_contents) = page_object.get_dict_value("Contents") else{
            return None;
        };
        let Some(content_ids) = page_contents.get_usize_array() else{
            return None;
        };
        
        read_objects_text(self, content_ids, &fonts)
    }

    /// Returns a page, given a page number
    pub(crate) fn get_page_no(&mut self, page_nr : usize) -> Option<PdfVar>{
        let root = self.trailer.root;
        let mut page_ids : Vec<usize> = Vec::new();

        // Get catalog object
        let Some(catalog_obj) = self.get_object_by_id(root) else {
            return None;
        };
        
        // Get pages object
        let Some(pages_obj) = catalog_obj.get_dict_value("Pages") else{
            return None;
        };
        
        // Get index of pages object
        let Some(pages_id) = pages_obj.get_indirect_obj_index() else{
            return None;
        };

        get_page_ids(self, &mut page_ids, pages_id);
        let Some(page_x) = page_ids.get(page_nr) else{
            return None;
        };
        let Some(o) = self.get_object_by_id(*page_x) else{
            return None;
        };
        Some(o)
    }

    /// Returns an object with given id
    /// Unpacks the object if it is in an object stream
    pub(crate) fn get_object_by_id(&mut self, obj_id : usize) -> Option<PdfVar>{
        // Fetch the object index in the xref table
        let (mut xref_1, mut compr_1) = {
            let Some(obj_ref) = self.xref.get(obj_id) else{
                return None;
            };
            (obj_ref.xref, obj_ref.compressed)
        };
        
        // If the object is compressed, decompress it
        if compr_1 != 1{
            unpack_obj_stm(self, xref_1);
            let Some(obj_xref) = self.xref.get(obj_id) else{
                return None;
            };
            xref_1 = obj_xref.xref;
            compr_1 = obj_xref.compressed;
            
            // If object is still compressed
            if compr_1 != 1{
                return None;
            }
        }

        // Parse object
        let object = match PdfVar::from(self, xref_1) {
            Ok(x) => x,
            Err(_) =>{
                return None;
            }
        };

        Some(object)
    }

    /// Returns true if document is encrypted
    pub fn is_encrypted(&mut self) -> bool{
        self.trailer.encrypt != 0
    }

    /// Returns a value from the documents info directory, based on a given key
    pub fn get_info(&mut self, key : &str) -> Option<String>{
        let info_ref = self.trailer.info;
        let info_obj = self.get_object_by_id(info_ref)?;
        let info_entry_obj = info_obj.get_dict_value(key)?;
        info_entry_obj.get_str()
    }

    pub(crate) fn from(filepath : &str) -> Result<Self, PdfError>{
        let doc_u8: Vec<u8> = match load_document(&filepath) {
            Ok(d) => d,
            Err(_e) => {
                return Err(PdfError::LoadError);
            }
        };
        let xref_table : Vec<ObjectRef> = Vec::new();
        let trailer : Trailer = Trailer { info: 0, root: 0, size: 0, encrypt: 0 };
        let mut doc = Document{xref : xref_table, trailer, data : doc_u8, it : 0};
        
        // Step 1: Look at head, Look for %PDF
        parse_pdf_version(&mut doc)?;

        // Step 2: Locate and parse startxref at the end of the file
        let startxref = read_start_xref(&mut doc)?;
        
        // Step 3: Parse the xref-table pointed at by startxref
        parse_xref(&mut doc, startxref, true)?;

        Ok(doc)
    }

    /// Returns the size of the reader data, in number of bytes
    pub fn size(&self) -> usize{
        self.data.len()
    }

    /// Returns the u8 char value at the current position in the reader. 
    pub(crate) fn byte(&self)->u8{
        return self.data[self.it];
    }

    /// Moves the documents position (it) forward until first non whitespace char 
    pub(crate) fn skip_whitespace(&mut self){
        self.it = skip_whitespace(&self.data, self.it);
    }

    /// Moves the position (it) to a new non empty line
    pub(crate) fn next_line(&mut self){
        while !matches!(self.byte(), 10 | 13){
            self.it += 1;
        }
        while matches!(self.byte(), 10 | 13){
            self.it += 1;
        }
    }
}


/// Iterates over vector from ix, returns first non whitespace index
pub(crate) fn skip_whitespace(doc_u8 : &Vec<u8>, start: usize) -> usize{
    let mut it = start;
    while doc_u8[it].is_ascii_whitespace(){
        it += 1;
    }
    return it;
}

/// Reads one pdf document
pub(crate) fn read_one_pdf(filepath : &str) -> Result<Document,PdfError>{
    let pdf = Document::from(filepath)?;
    Ok(pdf)
}

/// Reads and returns startxref-value from end of PDF
fn read_start_xref(doc : &mut Document) -> Result<usize,PdfError>{
    let mut startxref : usize = 0;
    doc.it = doc.size() -1;

    // Locate startxref at end of document
    loop {
        if doc.size()-doc.it > 100 || doc.it == 0{
            return Err(PdfError::XrefError);
        }
        if doc.byte() == b's'{
            if cmp_u8(&doc.data, doc.it, U_STARTXREF){
                doc.it+=9;
                break;
            }
            return Err(PdfError::XrefError);
        }
        doc.it -= 1;
    }
    doc.skip_whitespace();

    // Parse number
    while doc.byte().is_ascii_digit() {
        startxref = startxref*10+(doc.byte() as usize)-48;
        doc.it += 1;   
    }

    if startxref == 0{
        return Err(PdfError::XrefError);
    }
    Ok(startxref)
}

/// Confirms that file begins with %PDF
fn parse_pdf_version(doc : &mut Document) -> Result<(),PdfError>{
    if !cmp_u8(&doc.data, 0, U_PDF){
        return Err(PdfError::DocumentError);
    }
    Ok(())
}

// Reads the file given in filepath
fn load_document(filepath : &str) -> Result<Vec<u8>, io::Error>{
    let doc = match fs::read(filepath) {
        Ok(file) => file,
        Err(error) =>{
            return Err(error);
        }
    };
    Ok(doc)
}

/// Parse Xref objects and tables
fn parse_xref(doc : &mut Document, start : usize, createtrailer : bool) -> Result<(),PdfError>{
    doc.it = start;

    if doc.size() <= doc.it{
        return Err(PdfError::XrefError);
    }

    if doc.byte().is_ascii_digit(){
        // The case were the XREF is an object
        let start = doc.it;
        let xref_object = PdfVar::from(doc, start)?;

        // Decode stream and parse contents
        parse_xref_object(doc, &xref_object)?;

        if createtrailer{
            create_trailer(doc, &xref_object);
        }
    } else if doc.byte() == b'x'{
        // The case were the XREF is only and XREF table, and the trailer is expected after it
        parse_xref_table(doc)?;
        parse_table_trailer(doc, createtrailer)?;
    } else{
        return Err(PdfError::XrefError);
    }
    Ok(())
}

fn parse_table_trailer(doc : &mut Document, createtrailer : bool) -> Result<(),PdfError>{
    // Parse trailer. First verify that next is trailer
    doc.skip_whitespace();
    if !cmp_u8(&doc.data, doc.it, U_TRAILER){
        return Err(PdfError::XrefError);
    }
    doc.it += 7;
    let mut stack : Vec<PdfVar> = Vec::new();
    parse_object(doc, &mut stack)?;
    if stack.len() != 1{
        return Err(PdfError::XrefError);
    }
    let Some(trailer_dict) = stack.get(0) else {
        return Err(PdfError::XrefError);
    };

    
    if let Some(prev_obj) = trailer_dict.get_dict_value(C_PREV){
        match prev_obj.get_indirect_obj_index() {
            Some(x) => parse_xref(doc, x, false)?,
            None => return Err(PdfError::XrefError),
        };
    };
    
    if createtrailer{
        create_trailer(doc, trailer_dict);
    }
    Ok(())
}

/// Parses an xref table
fn parse_xref_table(doc : &mut Document) -> Result<(),PdfError>{
    if !cmp_u8(&doc.data, doc.it, U_XREF){
        return Err(PdfError::XrefError);
    }
    doc.it += 4;
    doc.skip_whitespace();
    if !doc.byte().is_ascii_digit(){
        return Err(PdfError::XrefError);
    }

    loop {
        let Some((index, _size1)) = read_number(doc) else{
            break;
        };

        // Skip space
        while doc.byte() == b' ' {
            doc.it += 1;
        }
        
        let Some((length, _size2)) = read_number(doc) else{
            return Err(PdfError::XrefError);
        };
        
        // Adjust size of xref table
        let size = index+length;
        for _ in doc.xref.len()..size{
            doc.xref.push(ObjectRef { compressed: 3, xref : 0, _version:0});
        }

        // Read the xref-entries
        for i in index..index+length {
            doc.next_line();
            let Some((num1, num1_size)) = read_number(doc) else{
                return Err(PdfError::XrefError);
            };
            if num1_size != 10{
                return Err(PdfError::XrefError);
            }
            doc.xref[i] = ObjectRef { compressed: 1, xref : num1, _version:0};
        }
        doc.next_line();
    }
    Ok(())
}

/// Reads a number, returns (number,numbersize)
fn read_number(doc : &mut Document) -> Option<(usize,usize)>{
    if !doc.byte().is_ascii_digit(){
        return None;
    }
    let mut size = 0;
    let mut num = 0;
    while doc.byte().is_ascii_digit() {
        num = num*10 + (doc.byte() - 48) as usize;
        doc.it += 1;
        size += 1;
    }

    return Some((num,size));
}

/// Parse the stream of an xref object object
fn parse_xref_object(doc : &mut Document, xref_object : &PdfVar) -> Result<(),PdfError>{
    // Get decoded stream from xref_object
    let Some(decoded) = xref_object.get_decoded_stream(doc) else{
        return Err(PdfError::XrefError);
    };

    // Fetch W, Size and Index
    // W = [1 2 1], How many bytes are in each column
    let w = match xref_object.get_dict_value("W"){
        Some(x) => match x.get_usize_array(){
            Some(x) => x,
            None => {
                return Err(PdfError::XrefError)
            },
        }
        None => {
            return Err(PdfError::XrefError);
        }
    };
    
    // Size, the total number of objects (in pdf, depending on which Xref)
    let size = match xref_object.get_dict_value(C_SIZE){
        Some(x) => match x.get_indirect_obj_index(){
            Some(x) => x,
            None => {
                return Err(PdfError::XrefError);
            }
        }
        None => {
            return Err(PdfError::XrefError);
        }
    };
    
    // Index = [3 27] or [641 3 648 4], default = [0 size]. The indexes + sizes for the objects this xref covers
    let index = match xref_object.get_dict_value("Index"){
        Some(x) => match x.get_usize_array(){
            Some(x) => x,
            None =>{
                return Err(PdfError::XrefError)
            }
        }
        None => vec![0,size],
    };
    
    // Declare variables
    let mut decoded_pos : usize = 0;
    let mut iw : usize = 0;
    let cols : usize = w.iter().sum();
    
    // Create new entries in xref_table if needed
    for _ in doc.xref.len()..size{
        doc.xref.push(ObjectRef { compressed: 3, xref : 0, _version:0});
    }
    
    // Has to be even, given [index size index size...]- pattern
    if index.len()%2 != 0{
        return Err(PdfError::XrefError);
    }
    
    // Interpret the decoded byte stream as xref
    while iw < index.len(){
        let object_index = index[iw];
        let list_size = index[iw+1];

        for i in object_index..object_index+list_size {
            // If object has already been read.
            if doc.xref[i].compressed != 3{
                decoded_pos += cols;
                continue;
            }

            if decoded_pos + cols > decoded.len(){
                return Err(PdfError::XrefError);
            }

            let w1 = get_256_repr(&decoded[decoded_pos..decoded_pos+w[0]]);
            let w2 = get_256_repr(&decoded[decoded_pos+w[0]..decoded_pos+w[0]+w[1]]);
            let w3 = get_256_repr(&decoded[decoded_pos+w[0]+w[1]..decoded_pos+cols]);
            doc.xref[i] = ObjectRef{compressed : w1 as u8, xref : w2, _version : w3};
            decoded_pos += cols;
        }

        iw += 2;
    }
    
    // If the xref contains a /Prev-key, read that previous Xref table
    if let Some(prev_obj) = xref_object.get_dict_value(C_PREV){
        if let Some(prev) = prev_obj.get_indirect_obj_index(){
            parse_xref(doc, prev, false)?;
        };
    };

    Ok(())
}


/// Creates and stores the PDF-trailer
fn create_trailer(doc : &mut Document, xref_obj : &PdfVar){
    let fields = ["Info", "Root", C_SIZE, "Encrypt"];
    let mut cnt : Vec<usize> = Vec::new();
    for f in fields{
        match xref_obj.get_dict_value(f) {
            Some(x) => {
                cnt.push(
                    match x.get_indirect_obj_index() {
                        Some(y) => y,
                        None => 0,   
                    }    
                );
            }
            None =>{
                cnt.push(0);
            }
        }
    }
    doc.trailer.info = cnt[0];
    doc.trailer.root = cnt[1];
    doc.trailer.size = cnt[2];
    doc.trailer.encrypt = cnt[3];
}


/// Adds found page id:s to the page_ids vector
fn get_page_ids(doc : &mut Document, page_ids : &mut Vec<usize>, obj_id : usize){
    // Fetch object
    let Some(object) = doc.get_object_by_id(obj_id) else{
        return;
    };
    
    // Fetch object type
    let Some(obj_type) = object.get_dict_value("Type") else{
        return;
    };
    
    // Fetch type as a string
    let Some(obj_name) = obj_type.get_name() else{
        return;
    };

    match obj_name.as_str() {
        "Pages" => {
            let Some(kids_obj) = object.get_dict_value("Kids") else{
                return;
            };
            let Some(kids_ids) = kids_obj.get_usize_array() else {
                return;
            };
            for kid in kids_ids{
                get_page_ids(doc, page_ids, kid);
            }
        }
        "Page" => {
            page_ids.push(obj_id);
        }
        _ => {
            // Unknown type
        }
    };
}

/// Tris to decode an ObjStm and append the decoded values to the document
/// Updates the xref-table for the objects in the ObjStm
fn unpack_obj_stm(doc : &mut Document, obj_id : usize){
    let Some(stream_obj) = doc.get_object_by_id(obj_id) else{
        return;
    };
    
    // Handle extends, when one ObjStm refers to another one
    if let Some(ext_obj) = stream_obj.get_dict_value("Extends"){
        let Some(ext_id) = ext_obj.get_indirect_obj_index() else {
            return;
        };
        unpack_obj_stm(doc, ext_id);
    };

    // First value
    let Some(first) = stream_obj.get_dict_int("First") else{
        return;
    };

    let Some(stream_decompr) = stream_obj.get_decoded_stream(doc) else{
        return;
    };

    let mut ix : usize = 0;
    let mut obj_nums : Vec<usize> = Vec::new();
    while ix < first{
        // Skip whitespace
        while stream_decompr[ix].is_ascii_whitespace(){
            ix += 1;
        }
        
        if !stream_decompr[ix].is_ascii_digit(){
            break;
        }

        // Parse number
        let mut num = 0;
        while stream_decompr[ix].is_ascii_digit() {
            num = num*10 + (stream_decompr[ix] - 48) as usize;
            ix += 1;
        }

        obj_nums.push(num);
    }
    
    ix = 1;
    while ix < obj_nums.len(){
        // Declare var
        let xref_start = doc.size();
        let ix_obj_id = obj_nums[ix-1];
        let obj_start = obj_nums[ix]+first;

        // The xref for obj_id should point to this ObjStm, otherwise we dont care
        if let Some(obj_xref) = doc.xref.get(ix_obj_id){
            if obj_xref.xref != obj_id {
                ix += 2;
                continue;
            }
        };

        // Read forward to get where the object ends
        let mut obj_end = stream_decompr.len();
        if ix +2 < obj_nums.len(){
            obj_end = obj_nums[ix+2]+first;
        }
        
        // Append object to documents byte vector
        let id_bytes : Vec<u8> = ix_obj_id.to_string().bytes().collect();
        doc.data.extend(id_bytes);
        doc.data.extend(b" 0 obj\n");
        doc.data.extend_from_slice(&stream_decompr[obj_start..obj_end]);
        doc.data.extend(b"endobj\n\n");

        doc.xref[ix_obj_id] = ObjectRef { compressed: 1, xref : xref_start, _version:0};
        ix += 2;
    }
}


