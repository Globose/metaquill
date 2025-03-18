use reqwest;
use serde_json::Value;
use crate::metadata::PDFStruct;
use std::error::Error;
use urlencoding::encode; // Import URL encoding

pub async fn call(pdf_metadata: &PDFStruct) -> Result<(), Box<dyn Error>> {
    let title_query = encode(&pdf_metadata.title.trim());
    let binding = "".to_string();
    let author_query = encode(pdf_metadata.author.get(0).unwrap_or(&binding).trim());

    let request_url = if author_query.is_empty() || author_query == "N/A" {
        format!("https://api.crossref.org/works?query.bibliographic={}", title_query)
    } else {
        format!(
            "https://api.crossref.org/works?query.bibliographic={}&query.author={}",
            title_query, encode(&author_query)
        )
    };

    println!("🔍 API Request URL: {}", request_url);

    let response = reqwest::get(&request_url).await?.text().await?;
    let json: Value = serde_json::from_str(&response)?;

    if json["status"] != "ok" {
        eprintln!(" Crossref API returned an error: {:?}", json);
        return Err("Crossref API Error".into());
    }

    let total_results = json["message"]["total-results"].as_i64().unwrap_or(0);
    if total_results == 0 {
        eprintln!("No results found. OCR might be needed.");
        return Err("No metadata found. Try OCR extraction.".into());
    }

    if let Some(items) = json["message"]["items"].as_array() {
        for work in items {
            let title = work["title"][0].as_str().unwrap_or("Unknown Title");
            let doi = work["DOI"].as_str().unwrap_or("No DOI available");
            let score = work["score"].as_f64().unwrap_or(0.0); // ✅ Extract confidence score
            let publisher = work["publisher"].as_str().unwrap_or("Unknown Publisher");
            let journal = work["container-title"][0].as_str().unwrap_or("Unknown Journal");
            let volume = work["volume"].as_str().unwrap_or("N/A");
            let issue = work["issue"].as_str().unwrap_or("N/A");
            let pages = work["page"].as_str().unwrap_or("N/A");
            let issn = work["ISSN"][0].as_str().unwrap_or("No ISSN");
            let url = work["URL"].as_str().unwrap_or("No URL available");

            let year = work["published-print"]["date-parts"][0][0]
                .as_i64()
                .or_else(|| work["published-online"]["date-parts"][0][0].as_i64())
                .unwrap_or(0);

            let authors = work["author"]
                .as_array()
                .map(|authors| {
                    authors
                        .iter()
                        .map(|a| {
                            format!(
                                "{} {}",
                                a["given"].as_str().unwrap_or(""),
                                a["family"].as_str().unwrap_or("")
                            )
                        })
                        .collect::<Vec<String>>()
                        .join(", ")
                })
                .unwrap_or("Unknown Authors".to_string());

            println!("Title: {}", title);
            println!("DOI: {}", doi);
            println!("Confidence Score: {:.2}", score);
            println!("Authors: {}", authors);
            println!("Publisher: {}", publisher);
            println!("Journal: {}", journal);
            println!("Year: {}", year);
            println!("Volume: {} | Issue: {} | Pages: {}", volume, issue, pages);
            println!("ISSN: {}", issn);
            println!("URL: {}", url);
            println!("-----------------------------------");
        }
    } else {
        println!("No results found.");
    }

    Ok(())
}