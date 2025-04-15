use crate::{document::Document, print_raw};






pub fn read_page_content(doc : &mut Document, obj_ids : Vec<usize>){
    println!("Reading ids {:?}", obj_ids);

    for obj_id in obj_ids{
        let Ok(obj) = doc.get_object_by_id(obj_id) else{
            return;
        };
        let Some(decoded) = obj.get_decoded_stream(doc) else {
            return;
        };
        print_raw(&decoded, 0, decoded.len());

    }

    println!("---");
    // println!("Obj {:?}", decoded);
}