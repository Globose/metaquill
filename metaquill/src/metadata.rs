use lopdf::{Document, Object,content::Content};
use regex::Regex;
use encoding_rs::WINDOWS_1252;
use tag_pdf_to_text::document;

struct TextObject{
    font_size: f32,
    text: String,
}

/// Save the PDF information
#[derive(Debug)]
pub struct PDFStruct {
    pub path: String,
    pub metadata_title: String,
    pub assumed_title: String,
    pub author: Vec<String>,
}

pub fn decode_bytes(bytes: &[u8]) -> String {
    let (cow, _, _) = WINDOWS_1252.decode(bytes); // Decode using Windows-1252
    let s = cow.to_string();
    if s.trim().is_empty() {
        "N/A".to_string()
    } else {
        return s;
    }
}

/// Returns the metadata for a given PDF document
pub fn fetch_metadata(document : &Document, filepath : &str) -> PDFStruct{
    // Create a struct for metadata
    let mut metadata = PDFStruct {
        path: filepath.to_string(),
        metadata_title: String::new(),
        assumed_title: String::new(),
        author: Vec::new(),
    };
    
    // Extract metadata from the file header
    collect_title_and_author(&document, &mut metadata);
    
    // Read assumed title
    metadata.assumed_title = text_to_metadata(&document);

    return metadata;
}

/// Returns the metadata for a given PDF document
pub fn extract_metadata(pdf : &mut document::Document, filepath : &str) -> PDFStruct{
    let mut meta_title = String::new();
    let mut meta_authors : Vec<String> = Vec::new();

    // Get title
    if let Some(title) = pdf.get_info("Title"){
        meta_title = title;
    };

    // Get title from text
    let assumed_title = get_probable_title(pdf);
    
    // Get authors
    if let Some(authors) = pdf.get_info("Author"){
        meta_authors = split_authors(&authors);
    };
    
    PDFStruct{path : filepath.to_string(), metadata_title : meta_title, assumed_title : assumed_title, author : meta_authors}
}

/// Collects the Title and Author from the PDF's trailer "Info" dictionary.
fn collect_title_and_author(document: &Document, metadata: &mut PDFStruct) {
    // Get the "Info" entry from the trailer, if available.
    if let Ok(Some(Object::Dictionary(dict))) = document.trailer.get(b"Info").map(|obj| match obj {
        Object::Reference(id) => document.get_dictionary(*id).ok().map(|d| Object::Dictionary(d.clone())),
        Object::Dictionary(d) => Some(Object::Dictionary(d.clone())),
        _ => None,
    }) {
        // Extract and decode Title
        metadata.metadata_title = dict
            .get(b"Title")
            .and_then(|v| v.as_str())
            .map(decode_bytes)
            .unwrap_or_else(|_| "N/A".to_string());

        // Extract and decode Author
        metadata.author = dict
            .get(b"Author")
            .and_then(|v| v.as_str())
            .map(|s: &[u8]| split_authors(&decode_bytes(s)))
            .unwrap_or_else(|_| vec!["N/A".to_string()]);

    } else {
        // Set default values if no Info dictionary is found
        metadata.metadata_title = "N/A".to_string();
        metadata.author = vec!["N/A".to_string()];
    }
}

/// Split the author string into individual names
fn split_authors(input: &str) -> Vec<String> {
    let re = Regex::new(r",|;|\band\b|&").unwrap(); // Split on comma, semicolon, "and" (whole word), or ampersand
    re.split(input)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Returns the most probable title from a list of Text elements
pub fn get_probable_title(pdf : &mut document::Document) -> String{
    let Some(mut texts) = pdf.get_text_from_page(0) else {
        return String::new();
    };
    if texts.len() == 0{
        return String::new();
    }
    
    // for t in &texts {
    //     println!("---");
    //     println!("{}", t.chars);
    //     println!("{}", t.scaled_font_size);
    //     println!("{}", t.avg_font_size);
    //     println!("{}", t.pos_y);
    // }

    // Keep all texts that can be accepted as a title
    texts.retain(|txt| txt.avg_font_size > 5.0);
    // texts.retain(|txt| txt.pos_y > 400.0); // Test this
    texts.retain(|txt| is_accepted_title(&txt.chars));
    
    // Find the largest text size
    let mut max : f64 = 0.0;
    for txt in &texts {
        if txt.avg_font_size > max {
            max = txt.avg_font_size;
        }
    }
    let max_lim = max * 0.9;
    
    // Remove everything smaller than 85% of the max text size
    texts.retain(|txt| txt.avg_font_size > max_lim || txt.avg_font_size > 13.0);
    
    // If the largest font is less than 11, return the first element
    if max < 11.0 {
        return match texts.get(0) {
            Some(x) => {
                x.chars.clone()
            }    
            None => String::new()
        }
    }

    // Otherwise, return the longest element
    let Some(longest_text) = texts.iter().max_by_key(|txt| txt.chars.len()) else {
        return String::new();
    };
    
    return longest_text.chars.clone();
}

/// Assumes a title based on the text in the pdf
pub fn text_to_metadata(doc: &Document) -> String{
    let mut text_objects : Vec<TextObject> = Vec::new();
    let mut current_font_size_value : f32 = 1.0;
    let mut text_scaler = 1.0;
    let mut current_real_font_size = 1.0;

    // Load Object data for page 1
    let Some(&page_id) = doc.get_pages().get(&1) else{
        println!("Failed to get page id for page 1");
        return "".to_string();
    };

    // Fetch content from page 1
    let Ok(page_content) = doc.get_page_content(page_id) else {
        println!("Failed to get page content");
        return "".to_string();
    };

    let Ok(content) = Content::decode(&page_content) else{
        println!("Failed to decode content");
        return "".to_string();
    };

    // Iterate over all objects on the page
    for operation in content.operations {
        match operation.operator.as_ref() {
            "Tf" => { // Change of font settings
                // let Some(font_ref) = operation.operands.get(0) else {
                //     continue;
                // };
                let Some(font_size_obj) = operation.operands.get(1) else{
                    continue;
                };

                let Ok(new_font_size_value) = font_size_obj.as_float() else{
                    continue;
                };
                current_font_size_value = new_font_size_value;

                // calculate new real font size
                let new_real_font_size = current_font_size_value * text_scaler;
                if current_real_font_size == new_real_font_size {
                    continue;
                }
                current_real_font_size = new_real_font_size;

                let new_text_object = TextObject{
                    font_size: current_real_font_size,
                    text: String::new(),
                };
                text_objects.push(new_text_object);
            }
            "Tj" => { // A text section
                let Some(text_obj) = operation.operands.get(0) else{
                    continue;
                };
                let Ok(char_array) = text_obj.as_str() else{
                    continue;
                };
                
                // decode text
                let (decoded, _, _) = WINDOWS_1252.decode(char_array);
                    
                if let Some(last_obj) = text_objects.last_mut(){
                    last_obj.text += &decoded;
                } 

            } // An array of text content
            "TJ" => {
                let Some(array_obj) = operation.operands.get(0) else{
                    continue;
                };
                let Ok(array) = array_obj.as_array() else{
                    continue;
                };
                for item in array{
                    let Ok(char_array) = item.as_str() else{
                        let Ok(spacing_value) = item.as_float() else{
                            continue;
                        };
                        if spacing_value.abs() > 150.0 {
                            if let Some(last_obj) = text_objects.last_mut(){
                                last_obj.text += " ";
                            } 
                        }
                        continue;
                    };

                    let (decoded, _, _) = WINDOWS_1252.decode(char_array);
                        
                    if let Some(last_obj) = text_objects.last_mut(){
                        last_obj.text += &decoded;
                    } 
                }
            }
            "Tm" =>{ // Change in scaling
                text_scaler = operation.operands[3].as_float().unwrap_or(1.0);

                // calculate new real font size
                let new_real_font_size = current_font_size_value * text_scaler;
                if current_real_font_size == new_real_font_size {
                    continue;
                }
                current_real_font_size = new_real_font_size;

                let new_text_object = TextObject{
                    font_size: current_real_font_size,
                    text: String::new(),
                };
                text_objects.push(new_text_object);
            }
            "TD" | "Tw" => { // Add space
                if let Some(last_obj) = text_objects.last_mut(){
                    last_obj.text += " ";
                } 
            }
            "Tc" => {
                // let tc = operation.operands[0].as_float().unwrap_or(1.0);
            }
            "TL" => {
                // let tl = operation.operands[0].as_float().unwrap_or(1.0);
                if let Some(last_obj) = text_objects.last_mut(){
                    last_obj.text += " ";
                }
            }
            "Td" => {
                // let tdx = operation.operands[0].as_float().unwrap_or(1.0);
                let tdy = operation.operands[1].as_float().unwrap_or(1.0);
                if tdy < -1.0{
                    if let Some(last_obj) = text_objects.last_mut(){
                        last_obj.text += " ";
                    }
                }
            }
            _ => { // Default
            }
        }
    }
    
    text_objects.sort_by(|x,y| y.font_size.partial_cmp(&x.font_size).unwrap());
    text_objects.retain(|txt_obj| txt_obj.text.len() > 17);
    text_objects.retain(|obj| !obj.text.contains("Authorized licensed use limited to"));
    
    if let Some(first_obj) = text_objects.first() {
        // println!("Assumed title: >{}<", first_obj.text);
        return first_obj.text.to_string();
    }
    return "".to_string();
}

/// Determines if a title is a title or not
pub fn is_accepted_title(title : &str) -> bool{
    if title.len() < 16 || title.len() > 300 {
        return false;
    }
    
    // Count each category of characters
    let mut letters = 0.0;
    let mut numbers = 0.0;
    let mut others = 0.0;
    let mut spaces = 0.0;
    for t in title.chars(){
        if t.is_ascii_alphabetic(){
            letters += 1.0;
        }
        else if t.is_ascii_digit() {
            numbers += 1.0;
        }
        else if t.is_whitespace(){
            spaces += 1.0;
        }
        else{
            others += 1.0;
        }
    }
    
    // A title has to contain at least one space
    if spaces < 1.0{
        return false;
    }
    
    let total_chars = letters + numbers + others;
    let avg_wlen = total_chars / spaces;
    
    // Average word length has to be below 14
    if avg_wlen > 14.0{
        return false;
    }
    
    // Number of non-space chars has to be greater than 14
    if total_chars < 14.0{
        return false;
    }
    
    let _o_oth = others/total_chars;
    let o_let = letters/total_chars;
    
    // Non-alphabetic chars musn't make up more than 30% of the title
    if o_let < 0.7{
        return false;
    }
    return true;
}
