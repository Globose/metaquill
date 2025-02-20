use load::load_pdf;
use std::env;

mod load;

fn main() {
    // Collect arguments
    let args : Vec<String> = env::args().collect();
    
    // temporary, more args will be accepted later on
    if args.len() != 2{
        println!("Failed to read PDF: No pdf given");
        return;
    }
    
    // Load PDF file
    let filepath : String = args[1].clone();
    let document = match load_pdf(&filepath){
        Ok(doc) => {
            println!("Document successfully loaded");
            doc
        }
        Err(e) => {
            println!("Failed to load PDF: {e}");
            return;
        }
    };

    // Print page count
    println!("PDF Page count: {}", document.get_pages().len());
}
