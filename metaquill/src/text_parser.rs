use lopdf::{content::Content, Document};
use log:: info;
use encoding_rs::WINDOWS_1252;

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::sync::OnceLock;

static ELSEVIER_SET: OnceLock<HashSet<String>> = OnceLock::new();

struct TextObject{
    order: u32,
    font_size: f32,
    text: String,
}

pub fn text_to_metadata(doc: &Document) -> String{
    if ELSEVIER_SET.get().is_none() {
        init_journal_set("elsevier.txt");
    }

    // RUST_LOG=info cargo run

    let mut text_objects : Vec<TextObject> = Vec::new();
    let mut current_font_size_value : f32 = 1.0;
    let mut text_scaler = 1.0;
    let mut current_real_font_size = 1.0;
    let mut order_count = 0;

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
                let Some(font_ref) = operation.operands.get(0) else {
                    continue;
                };
                let Some(font_size_obj) = operation.operands.get(1) else{
                    continue;
                };
                info!("Tf, Len = {}, ref = {:?}, size = {:?}", operation.operands.len(), font_ref, font_size_obj);

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
                info!("New real font size: {}", current_real_font_size);

                let new_text_object = TextObject{
                    order: order_count,
                    font_size: current_real_font_size,
                    text: String::new(),
                };
                text_objects.push(new_text_object);
                order_count += 1;
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

                info!("Tj: str: {}", decoded);
            } // An array of text content
            "TJ" => {
                info!("--TJ--");
                let Some(array_obj) = operation.operands.get(0) else{
                    continue;
                };
                let Ok(array) = array_obj.as_array() else{
                    continue;
                };
                info!("{:?}", array);
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
                info!("Tm, Oplen = {}", operation.operands.len());
                text_scaler = operation.operands[3].as_float().unwrap_or(1.0);

                // calculate new real font size
                let new_real_font_size = current_font_size_value * text_scaler;
                if current_real_font_size == new_real_font_size {
                    continue;
                }
                current_real_font_size = new_real_font_size;
                info!("Scaled new font size: {}", current_real_font_size);

                let new_text_object = TextObject{
                    order: order_count,
                    font_size: current_real_font_size,
                    text: String::new(),
                };
                text_objects.push(new_text_object);
                order_count += 1;
            }
            "TD" | "Tw" => { // Add space
                info!("Line Break {}", operation.operator);
                info!("{:?}", operation.operands);
                if let Some(last_obj) = text_objects.last_mut(){
                    last_obj.text += " ";
                } 
            }
            "Tc" => {
                info!("Tc: {}", operation.operands.len());
                let tc = operation.operands[0].as_float().unwrap_or(1.0);
                info!("t_info: {}", tc);
            }
            "TL" => {
                info!("TL: {}", operation.operands.len());
                let tl = operation.operands[0].as_float().unwrap_or(1.0);
                info!("tl_info: {}", tl);
                if let Some(last_obj) = text_objects.last_mut(){
                    last_obj.text += " ";
                }
            }
            "Td" => {
                info!("Td: {}", operation.operands.len());
                let tdx = operation.operands[0].as_float().unwrap_or(1.0);
                let tdy = operation.operands[1].as_float().unwrap_or(1.0);
                info!("td_info: {}, {}", tdx, tdy);

                // if tdx > 0.8{
                //     if let Some(last_obj) = text_objects.last_mut(){
                //         last_obj.text += " ";
                //     }
                // }
                if tdy < -1.0{
                    if let Some(last_obj) = text_objects.last_mut(){
                        last_obj.text += " ";
                    }
                }
            }
            _ => { // Default
                info!("U - {}, size = {}", operation.operator, operation.operands.len());
            }
        }
    }
    
    info!("No. of text_objects = {}", text_objects.len());
    text_objects.sort_by(|x,y| y.font_size.partial_cmp(&x.font_size).unwrap());
    text_objects.retain(|txt_obj| txt_obj.text.len() > 17);
    text_objects.retain(|txt_obj| !contains_journal(&txt_obj.text));
    text_objects.retain(|obj| !obj.text.contains("Authorized licensed use limited to"));
    
    // text_objects.retain(|txt_obj| txt_obj.order < 10);
    for txt_obj in &text_objects{
        info!("---");
        info!("{}", txt_obj.order);
        info!("{}", txt_obj.font_size);
        info!("{}", txt_obj.text.len());
        info!("{}", txt_obj.text);
    }
    
    if let Some(first_obj) = text_objects.first() {
        println!("Assumed title: >{}<", first_obj.text);
        return first_obj.text.to_string();
    }
    return "".to_string();
}

fn init_journal_set<P: AsRef<Path>>(filename: P) {
    let file = File::open(filename).expect("Failed to open file");
    let reader = io::BufReader::new(file);
    
    let set: HashSet<String> = reader.lines().filter_map(Result::ok).collect();
    ELSEVIER_SET.set(set).expect("Failed to initialize JOURNAL_SET");
}

// Funktion för att kolla om en tidskrift finns i setet
fn contains_journal(journal: &str) -> bool {
    let modified_str = journal.trim();
    ELSEVIER_SET.get().map_or(false, |set| set.contains(modified_str))
}
