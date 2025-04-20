use std::{fs::{self, read_dir}, io, string::ParseError};

use crate::{decoding::{decode_flate, decode_pdfdoc, decode_pdfdoc_u8, get_256_repr, png_decode}, pdf_object::{cmp_u8, parse_object, PdfParseError, PdfVar}, print_raw, text_parser::{get_page_resources, read_objects_text, Text}};

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
    pub data : Vec<u8>,
    pub it : usize,
}
// #[derive(Debug)]
// pub struct Reader{
//     pub data : Vec<u8>,
//     pub it : usize, // Current position
// }

#[derive(Debug)]
pub enum PdfDocError {
    CompressedObject,
    PdfParseError,
    KeyNotFound,
    XrefIndexError,
    TypeError,
    FileDirError,
    NotAnObject,
    EmptyObject,
    NotADictionary,
    XrefOutOfBounds,
}

const U_STARTXREF : &[u8] = b"startxref";
const U_PDF : &[u8] = b"%PDF";
// const U_STREAM : &[u8] = b"stream";
const U_TRAILER : &[u8] = b"trailer";
const U_XREF : &[u8] = b"xref";
// const C_EOF : &str = "%%EOF";
// const C_OBJ : &str = "obj";
// const C_ENDOBJ : &str = "endobj";
const C_PREV : &str = "Prev";
const C_ROOT : &str = "Root";
const C_INFO : &str = "Info";
const C_SIZE : &str = "Size";
const C_FILTER : &str = "Filter";
const C_FLATEDECODE : &str = "FlateDecode";
const C_LENGTH : &str = "Length";
const C_DECODEPARMS : &str = "DecodeParms";
const C_PREDICTOR : &str = "Predictor";
const C_COLUMNS : &str = "Columns";
const C_W : &str = "W";
const C_INDEX : &str = "Index";
const C_ENCRYPT : &str = "Encrypt";

impl Document {
    /// Read Text Sections From Page
    pub fn get_text_from_page(&mut self, page_nr : usize) -> Option<Vec<Text>>{
        let Some(page_object) = self.get_page_no(page_nr) else{
            return None;
        };
        
        // Get Page Fonts
        let fonts = get_page_resources(self, &page_object);
        // println!("Fonts {:?}", fonts);

        // Get ids of content
        let Some(page_contents) = page_object.get_dict_value("Contents") else{
            println!("No contents found");
            return None;
        };
        let Some(content_ids) = page_contents.get_usize_array(self) else{
            println!("Failed to get usize array contents");
            return None;
        };
        read_objects_text(self, content_ids, &fonts)
    }

    /// Returns a page
    pub fn get_page_no(&mut self, page_nr : usize) -> Option<PdfVar>{
        // println!("Get page {:?}", self.trailer);
        let root = self.trailer.root;
        let mut page_ids : Vec<usize> = Vec::new();

        let Ok(catalog_obj) = self.get_object_by_id(root) else {
            println!("Root obj does not exist");
            return None;
        };

        let Some(pages_obj) = catalog_obj.get_dict_value("Pages") else{
            println!("No Pages Found");
            return None;
        };

        let Some(pages_id) = pages_obj.get_indirect_obj_index(self) else{
            return None;
        };

        get_page_ids(self, &mut page_ids, pages_id);
        let Some(page_x) = page_ids.get(page_nr) else{
            return None;
        };
        let Ok(o) = self.get_object_by_id(*page_x) else{
            return None;
        };
        Some(o)
    }

    /// Returns an object with given id
    /// Unpacks the object if it is in an object stream
    pub fn get_object_by_id(&mut self, obj_id : usize) -> Result<PdfVar,PdfDocError>{
        // Fetch the object index in the xref table
        let (mut xref_1, mut compr_1) = {
            let Some(obj_ref) = self.xref.get(obj_id) else{
                return Err(PdfDocError::XrefOutOfBounds);
            };
            (obj_ref.xref, obj_ref.compressed)
        };
        
        // If the object is compressed, decompress it
        if compr_1 != 1{
            unpack_obj_stm(self, xref_1);
            let Some(obj_xref) = self.xref.get(obj_id) else{
                return Err(PdfDocError::XrefOutOfBounds);
            };
            xref_1 = obj_xref.xref;
            compr_1 = obj_xref.compressed;
            
            if compr_1 != 1{
                println!("Obj still not uncompressed");
                return Err(PdfDocError::CompressedObject);
            }
        }

        // Parse object
        let object = match PdfVar::from(self, xref_1) {
            Ok(x) => x,
            Err(e) =>{
                println!("Error {:?}", e);
                return Err(PdfDocError::PdfParseError);
            }
        };

        return Ok(object)
    }

    /// Returns true if document is encrypted
    pub fn is_encrypted(&mut self) -> bool{
        self.trailer.encrypt != 0
    }

    /// Returns a value from the documents info directory, based on a given key
    pub fn get_info(&mut self, key : &str) -> Result<String,PdfDocError>{
        let info_ref = self.trailer.info;
        
        let info_obj = match self.get_object_by_id(info_ref) {
            Ok(x) => x,
            Err(e) => return Err(e),
        };
        
        // Get info record
        let Some(info_entry_obj) = info_obj.get_dict_value(key) else{
            return Err(PdfDocError::KeyNotFound);
        };

        let Some(info_str) = info_entry_obj.get_str() else{
            return Err(PdfDocError::TypeError);
        };

        Ok(info_str)
    }

    pub fn from(filepath : &str) -> Result<Self, PdfParseError>{
        let doc_u8: Vec<u8> = match load_document(&filepath) {
            Ok(d) => d,
            Err(_e) => {
                return Err(PdfParseError::PdfLoadError);
            }
        };
        let mut xref_table : Vec<ObjectRef> = Vec::new();
        let mut trailer : Trailer = Trailer { info: 0, root: 0, size: 0, encrypt: 0 };
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
    pub fn byte(&self)->u8{
        return self.data[self.it];
    }

    /// Moves the documents position (it) forward until first non whitespace char 
    pub fn skip_whitespace(&mut self){
        self.it = skip_whitespace(&self.data, self.it);
    }

    /// Moves the position (it) to a new non empty line
    pub fn next_line(&mut self){
        while !matches!(self.byte(), 10 | 13){
            self.it += 1;
        }
        while matches!(self.byte(), 10 | 13){
            self.it += 1;
        }
    }
}


/// Iterates over vector from ix, returns first non whitespace index
pub fn skip_whitespace(doc_u8 : &Vec<u8>, start: usize) -> usize{
    let mut it = start;
    while doc_u8[it].is_ascii_whitespace(){
        it += 1;
    }
    return it;
}

/// Reads all the documents in a directory, including sub-directories. ONLY FOR TESTING
pub fn read_pdf_in_dir(filepath : &str) -> Result<(usize,usize),PdfDocError>{
    let mut pdfs_read = 0;
    let mut pdfs_accepted = 0;

    let dir = match read_dir(filepath){
        Ok(x) => x,
        Err(_e) => {
            return Err(PdfDocError::FileDirError);
        }
    };

    // Iterate over each directory entry
    for path in dir{
        // Control directory entry
        let Ok(dir_entry) = path else{
            continue;
        };

        // Load the file type
        let Ok(ftype) = dir_entry.file_type() else{
            continue;
        };

        let dir_entry_path = dir_entry.path();
        let Some(new_path) = dir_entry_path.as_os_str().to_str() else{
            continue;
        };

        // If directory entry is a directory, do a recursive call
        if ftype.is_dir(){
            if let Ok(pdf_cnt) = read_pdf_in_dir(new_path){
                pdfs_read += pdf_cnt.0;
                pdfs_accepted += pdf_cnt.1;
            }
        }
        else{
            match read_one_pdf(new_path) {
                Ok(mut pdf) => {
                    let mut author = String::new();
                    let mut title = String::new();
                    if let Ok(value) = pdf.get_info("Author"){
                        author = value;
                    };
                    if let Ok(value) = pdf.get_info("Title"){
                        title = value;
                    };
                    println!("{}", new_path);
                    // println!("A: {}, T: {}", author, title);

                    if let Some(text_objects) = pdf.get_text_from_page(0){
                        // println!("Textobjects {}", text_objects.len());
                        // for text_obj in text_objects{
                        //     println!("---");
                        //     println!("Pos Y: {}", text_obj.pos_y);
                        //     println!("Font size: {}", text_obj.scaled_font_size);
                        //     println!("Text (r): {:?}", text_obj.chars);
                        //     println!("Text: {}", text_obj.chars);
                        // }
                    }
                },
                Err(e) =>{
                    println!("Error when reading pdf {:?}", e);
                }
            }
            pdfs_read += 1;
        }
    }
    Ok((pdfs_read,pdfs_accepted))
}

/// Reads one pdf document
pub fn read_one_pdf(filepath : &str) -> Result<Document,PdfParseError>{
    let mut pdf = Document::from(filepath)?;
    Ok(pdf)
}

/// Reads and returns startxref-value from end of PDF
fn read_start_xref(doc : &mut Document) -> Result<usize,PdfParseError>{
    let mut startxref : usize = 0;
    doc.it = doc.size() -1;

    // Locate startxref at end of document
    loop {
        if doc.size()-doc.it > 100 || doc.it == 0{
            return Err(PdfParseError::StartXrefNotFound);
        }
        if doc.byte() == b's'{
            if cmp_u8(&doc.data, doc.it, U_STARTXREF){
                doc.it+=9;
                break;
            }
            return Err(PdfParseError::StartXrefNotFound);
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
        return Err(PdfParseError::StartXrefNotFound);
    }
    Ok(startxref)
}

/// Confirms that file begins with %PDF
fn parse_pdf_version(doc : &mut Document) -> Result<(),PdfParseError>{
    if !cmp_u8(&doc.data, 0, U_PDF){
        return Err(PdfParseError::PdfHeaderError);
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
fn parse_xref(doc : &mut Document, start : usize, createtrailer : bool) -> Result<(),PdfParseError>{
    doc.it = start;

    if doc.size() <= doc.it{
        return Err(PdfParseError::OutOfBounds);
    }

    if doc.byte().is_ascii_digit(){
        // The case were the XREF is an object
        let start = doc.it;
        let xref_object = PdfVar::from(doc, start)?;
        parse_xref_object(doc, &xref_object)?;

        if createtrailer{
            create_trailer(doc, &xref_object);
        }
    } else if doc.byte() == b'x'{
        // The case were the XREF is only and XREF table, and the trailer is expected after it
        parse_xref_table(doc)?;
        parse_table_trailer(doc, createtrailer)?;
    } else{
        return Err(PdfParseError::XrefError);
    }
    // println!("Trailer {:?}", doc.trailer);
    Ok(())
}

fn parse_table_trailer(doc : &mut Document, createtrailer : bool) -> Result<(),PdfParseError>{
    // Parse trailer. First verify that next is trailer
    doc.skip_whitespace();
    if !cmp_u8(&doc.data, doc.it, U_TRAILER){
        return Err(PdfParseError::TrailerNotFound);
    }
    doc.it += 7;
    let mut stack : Vec<PdfVar> = Vec::new();
    parse_object(doc, &mut stack)?;
    if stack.len() != 1{
        return Err(PdfParseError::TrailerError);
    }
    let Some(trailer_dict) = stack.get(0) else {
        return Err(PdfParseError::TrailerError);
    };

    
    if let Some(prev_obj) = trailer_dict.get_dict_value(C_PREV){
        match prev_obj.get_indirect_obj_index(doc) {
            Some(x) => parse_xref(doc, x, false)?,
            None => return Err(PdfParseError::TrailerError),
        };
    };
    
    if createtrailer{
        create_trailer(doc, trailer_dict);
    }
    Ok(())
}

/// Parses an xref table
fn parse_xref_table(doc : &mut Document) -> Result<(),PdfParseError>{
    if !cmp_u8(&doc.data, doc.it, U_XREF){
        return Err(PdfParseError::XrefTableError);
    }
    doc.it += 4;
    doc.skip_whitespace();
    if !doc.byte().is_ascii_digit(){
        return Err(PdfParseError::XrefTableError);
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
            return Err(PdfParseError::NumParseError);
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
                return Err(PdfParseError::NumParseError);
            };
            if num1_size != 10{
                return Err(PdfParseError::XrefTableError);
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

/// Parse xref object object
fn parse_xref_object(doc : &mut Document, xref_object : &PdfVar) -> Result<(),PdfParseError>{
    // Get decoded stream from xref_object
    let Some(decoded) = xref_object.get_decoded_stream(doc) else{
        return Err(PdfParseError::EmptyObject);
    };

    // Fetch W, Size and Index
    // W = [1 2 1], How many bytes are in each column
    // Size, the total number of objects (in pdf, depending on which Xref)
    // Index = [3 27] or [641 3 648 4], default = [0 size]. The indexes + sizes for the objects this xref covers
    let w = match xref_object.get_dict_value(C_W){
        Some(x) => match x.get_usize_array(doc){
            Some(x) => x,
            None => {
                return Err(PdfParseError::DecodeDataError)
            },
        }
        None => {
            return Err(PdfParseError::DecodeDataError);
        }
    };

    let size = match xref_object.get_dict_value(C_SIZE){
        Some(x) => match x.get_indirect_obj_index(doc){
            Some(x) => x,
            None => {
                return Err(PdfParseError::DecodeDataError);
            }
        }
        None => {
            return Err(PdfParseError::DecodeDataError);
        }
    };
    
    let index = match xref_object.get_dict_value(C_INDEX){
        Some(x) => match x.get_usize_array(doc){
            Some(x) => x,
            None =>{
                return Err(PdfParseError::DecodeDataError)
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
        return Err(PdfParseError::DecodeDataError);
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
                return Err(PdfParseError::DecodeDataError);
            }

            // println!("dec {:?}", decoded);
            let w1 = get_256_repr(&decoded[decoded_pos..decoded_pos+w[0]]);
            let w2 = get_256_repr(&decoded[decoded_pos+w[0]..decoded_pos+w[0]+w[1]]);
            let w3 = get_256_repr(&decoded[decoded_pos+w[0]+w[1]..decoded_pos+cols]);
            // println!("{}: {}, {}, {}", i, w1, w2, w3);
            doc.xref[i] = ObjectRef{compressed : w1 as u8, xref : w2, _version : w3};
            decoded_pos += cols;
        }

        iw += 2;
    }
    
    // If the xref contains a /Prev-key, read that previous Xref table
    if let Some(prev_obj) = xref_object.get_dict_value(C_PREV){
        if let Some(prev) = prev_obj.get_indirect_obj_index(doc){
            parse_xref(doc, prev, false)?;
        };
    };

    Ok(())
}


/// Creates and stores the PDF-trailer
fn create_trailer(doc : &mut Document, xref_obj : &PdfVar){
    let fields = [C_INFO, C_ROOT, C_SIZE, C_ENCRYPT];
    let mut cnt : Vec<usize> = Vec::new();
    for f in fields{
        // println!("Field {}", f);
        match xref_obj.get_dict_value(f) {
            Some(x) => {
                cnt.push(
                    match x.get_indirect_obj_index(doc) {
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


/// Returns a vector of page ids
fn get_page_ids(doc : &mut Document, page_ids : &mut Vec<usize>, obj_id : usize){
    // Fetch object
    let Ok(object) = doc.get_object_by_id(obj_id) else{
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
            let Some(kids_ids) = kids_obj.get_usize_array(doc) else {
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
            println!("Unknwon type {}", obj_name);
        }
    }
}

/// Decodes an ObjStm and appends the decoded values to the document
/// Updates the xref-table for the objects in the ObjStm
fn unpack_obj_stm(doc : &mut Document, obj_id : usize){
    let Ok(stream_obj) = doc.get_object_by_id(obj_id) else{
        return;
    };
    
    // Does not handle extends
    if let Some(_) = stream_obj.get_dict_value("Extends"){
        println!("Object Stm has extends");
        return;
    };

    // Get N and First value
    let Some(n_value) = stream_obj.get_dict_int("N", doc) else{
        println!("Dict to int fail");
        return;
    };
    
    let Some(first) = stream_obj.get_dict_int("First", doc) else{
        println!("First to int fail");
        return;
    };

    // println!("Fn = {}, {}", n_value, first);

    let Some(stream_decompr) = stream_obj.get_decoded_stream(doc) else{
        println!("Failed to decode object stream");
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
                println!("Obj id != xref");
                ix += 2;
                continue;
            }
        };

        // Read forward to get where the object ends
        let mut obj_end = stream_decompr.len();
        if ix +2 < obj_nums.len(){
            obj_end = obj_nums[ix+2]+first;
        }
        // println!("({},{},{})", ix_obj_id, obj_start, obj_end);
        // print_raw(&stream_decompr, obj_start, obj_end-obj_start);
        
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


