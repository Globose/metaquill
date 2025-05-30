MetaQuill

MetaQuill is a Rust-based tool designed to extract metadata from PDF files. It provides a command-line interface to process PDFs and retrieve structured metadata efficiently.

Features
- Extracts metadata from PDF documents
- Command-line interface for easy usage
- Built with Rust for performance and safety

Installation
To build and install MetaQuill, ensure you have Rust and Cargo installed:

git clone https://github.com/Globose/metaquill.git
cd metaquill
cargo build --release
The compiled binary will be located at target/release/metaquill.

Usage

./metaquill path/to/document.pdf
or
./metaquill path/to/directory
This command will output the extracted metadata from the specified PDF file or directory in a json file.

License
