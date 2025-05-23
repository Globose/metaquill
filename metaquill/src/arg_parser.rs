use crate::metadata::PdfStruct;

#[derive(Debug)]
pub struct PdfData {
    pub pdfs : Vec<PdfStruct>,
    pub read : u32,
    pub fails : u32,
    pub timeouts : u32,
    pub api_hits : u32,
    pub output_filepath : String,
    pub reader : u8, // 1 = lopdf, 0 standard
    pub make_api_call : bool,
    pub verbose : Verbose,
    pub recursive : bool,
    pub path : String,
}

#[derive(PartialEq, Debug)]
pub enum Verbose {
    Light,
    Default,
    Full,
}

pub fn parse_args(args : &Vec<String>) -> Option<PdfData>{
    if args.len() < 2 {
        println!("No filepath given");
        println!("Metaquill usage: ./metaquill [pdf filepath] [arguments]");
        println!("Use -help to show available argument options");
        return None;
    }

    if args[1] == "-help" || args[1] == "-h" {
        print_help();
        return None;
    }
    
    let mut pdf_data = PdfData{
        pdfs : Vec::new(), 
        read : 0, 
        fails : 0, 
        reader : 0, 
        output_filepath : "output.json".to_string(), 
        // print_info: true, 
        make_api_call : true,
        recursive: false, 
        api_hits : 0,
        verbose : Verbose::Default,
        path : args[1].to_string(),
        timeouts : 0,
    };

    let mut arg_it : usize = 2;

    // Iterate over all args
    while arg_it < args.len() {
        match args[arg_it].as_str() {
            "-h" | "-help" => {
                print_help();
                return None;
            }
            "-r" | "-reader" => {
                parse_reader(&mut arg_it, &mut pdf_data, args)?;
            }
            "-nc" | "-nocall" => {
                pdf_data.make_api_call = false;
            }
            "-o" | "-output" => {
                arg_it += 1;
                let Some(next_arg) = args.get(arg_it) else {
                    println!("No argument given for output");
                    println!("Use -help to show available argument options");
                    return None;
                };
                pdf_data.output_filepath = next_arg.to_string();
            }
            "-v" | "-verbose" => {
                parse_verbose(&mut arg_it, &mut pdf_data, args)?;
            }
            "-rec" | "-recursive" => {
                pdf_data.recursive = true;
            }
            _ => {
                println!("Unknown argument given: {}", args[arg_it]);
                println!("Metaquill usage: ./metaquill [pdf filepath] [arguments]");
                println!("Use -help to show available argument options");
                return None;
            }
        }
        arg_it += 1;
    }
    
    return Some(pdf_data);
}

fn print_help(){
    println!("Metaquill usage: ./metaquill [filepath] [arguments]");
    println!("Metaquill arguments:");
    println!("\t-h | -help — show help instructions");
    println!("\t-r | -reader — choose what reader to use [tag | lopdf] (default = 'tag') ");
    println!("\t-nc | -nocall — makes no api call");
    println!("\t-v | -verbose — choose verbose to run [light | default | full]");
    println!("\t-o | -output — set path for json output file (default = 'output.json')");
    println!("\t-rec | -recursive — search subdirectories if encountered");
}

/// Parses argument for -reader
fn parse_reader(it : &mut usize, pdf_data : &mut PdfData, args : &Vec<String>) -> Option<()>{
    // Find next arg
    *it += 1;
    let Some(next_arg) = args.get(*it) else {
        println!("No argument given for reader");
        println!("Use -help to show available argument options");
        return None;
    };
    
    match next_arg.as_str() {
        "tag" | "t" => {
            pdf_data.reader = 0;
        }
        "lopdf" | "l" => {
            pdf_data.reader = 1;
        }
        _ => {
            println!("Invalid argument for reader: {}", next_arg);
            println!("Use -help to show available argument options");
            return None;
        }
    }
    
    return Some(());
}

/// Parses argument for -verbose
fn parse_verbose(it : &mut usize, pdf_data : &mut PdfData, args : &Vec<String>) -> Option<()>{
    // Find next arg
    *it += 1;
    let Some(next_arg) = args.get(*it) else {
        println!("No argument given for verbose");
        println!("Use -help to show available argument options");
        return None;
    };
    
    match next_arg.as_str() {
        "full" | "f" => {
            pdf_data.verbose = Verbose::Full;
        }
        "light" | "l" => {
            pdf_data.verbose = Verbose::Light;
        }
        "default" | "d" => {
            pdf_data.verbose = Verbose::Default;
        }
        _ => {
            println!("Invalid argument for verbose: {}", next_arg);
            println!("Use -help to show available argument options");
            return None;
        }
    }
    
    return Some(());
}