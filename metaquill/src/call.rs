use reqwest;
use serde_json::Value;
use crate::metadata::PDFStruct;
use std::error::Error;
use urlencoding::encode; // Import URL encoding
use std::collections::VecDeque;

pub struct Metadata {
    pub title: String,
    pub doi: String,
    pub score: f64,
    pub authors: String,
    pub publisher: String,
    pub journal: String,
    pub year: i64,
    pub volume: String,
    pub issue: String,
    pub pages: String,
    pub issn: String,
    pub url: String,
}

/// Extracts a string field from JSON safely
fn extract_str(json: &Value, key: &str) -> String {
    json.get(key)
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string()
}

/// Extracts the first string from an array field
fn extract_array_str(json: &Value, key: &str) -> String {
    json.get(key)
        .and_then(|arr| arr.get(0))
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string()
}

/// Extracts authors in a compact format
fn extract_authors(json: &Value) -> String {
    json["author"]
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
        .unwrap_or_else(|| "Unknown Authors".to_string())
}



pub async fn call(pdf_metadata: &PDFStruct) -> Result<VecDeque<Metadata>, Box<dyn Error>>  {
    // Encodes the title to url format
    let title_query = encode(&pdf_metadata.title.trim());
    let binding = "".to_string();
    // Encodes the first author to url format
    let author_query = encode(pdf_metadata.author.get(0).unwrap_or(&binding).trim());

    // Request url with both title and author or only title
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

    // Check status of the response return error if not ok
    if json["status"] != "ok" {
        eprintln!(" Crossref API returned an error: {:?}", json);
        return Err("Crossref API Error".into());
    }

    // Check if any results are returned, if not return error to start ocr
    let total_results = json["message"]["total-results"].as_i64().unwrap_or(0);
    if total_results == 0 {
        return Err("No metadata found. Try OCR extraction.".into());
    }

    let mut metadata_list = VecDeque::new();

    if let Some(items) = json["message"]["items"].as_array() {
        for work in items {
            let metadata = Metadata {
                title: extract_array_str(work, "title"),
                doi: extract_str(work, "DOI"),
                score: work["score"].as_f64().unwrap_or(0.0),
                authors: extract_authors(work),
                publisher: extract_str(work, "publisher"),
                journal: extract_array_str(work, "container-title"),
                year: work["published-print"]["date-parts"]
                    .get(0)
                    .and_then(|arr| arr.get(0))
                    .and_then(Value::as_i64)
                    .or_else(|| {
                        work["published-online"]["date-parts"]
                            .get(0)
                            .and_then(|arr| arr.get(0))
                            .and_then(Value::as_i64)
                    })
                    .unwrap_or(0),
                volume: extract_str(work, "volume"),
                issue: extract_str(work, "issue"),
                pages: extract_str(work, "page"),
                issn: extract_array_str(work, "ISSN"),
                url: extract_str(work, "URL"),
            };

            metadata_list.push_back(metadata);
        }
    }

    Ok(metadata_list)
}