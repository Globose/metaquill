use lopdf::{Document, Error};

pub fn load_pdf(filepath : &str) -> Result<Document, Error> {
    println!("Loading PDF {filepath}");
    let document = Document::load(filepath)?;
    return Ok(document);
}
