use reqwest::Client;
use serde_json::Value;
use crate::metadata::PDFStruct;
use std::error::Error;
use urlencoding::encode; // Import URL encoding
use std::time::Duration;
use tokio::time::timeout;

use strsim::levenshtein; // Comparing two string

// Struct to collect all metadata
#[derive(Clone, Debug)]
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
    pub title_confidence: f64,
    pub file_path : String,
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

/// Use levenshein distance formula to determine similarity between strings
pub fn compare_results(result_title: &str, search_title: &str) -> f64 {
    // Normalize: lowercase and trim both strings
    let result = result_title.trim().to_lowercase();
    let search = search_title.trim().to_lowercase();

    if result.is_empty() || search.is_empty() {
        return 0.0;
    }

    let distance = levenshtein(&result, &search);
    let max_len = result.len().max(search.len()) as f64;
    //
    let confidence = ((1.0 - distance as f64 / max_len) * 100.0).max(0.0);
    return confidence;
}

pub async fn fetch_with_retry(request_url: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut attempts = 0;
    // Sets attempts to 2
    while attempts < 2 {
        attempts += 1;

        // Set max wait time (5 seconds)
        let result = timeout(Duration::from_secs(5), async {
            // API call
            let response = client.get(request_url).send().await?;
            let text = response.text().await?;
            Ok::<_, reqwest::Error>(text)
        })
        .await;

        match result {
            Ok(Ok(body)) => {
                let json: Value = serde_json::from_str(&body)?;
                if json["status"] != "ok" {
                    eprintln!("Crossref API returned an error: {:?}", json);
                    return Err("Crossref API Error".into());
                }
                return Ok(json);
            }
            Ok(Err(e)) => {
                eprintln!("Request failed: {}", e);
            }
            Err(_) => {
                eprintln!("Request timed out on attempt {}", attempts);
            }
        }

        // Retry once if first attempt fails
        if attempts < 2 {
            eprintln!("Retrying...");
        }
    }

    Err("Request failed after retry".into())
}


pub async fn call(pdf_metadata: &PDFStruct) -> Result<Option<Metadata>, Box<dyn Error>>  {
    // Add correct titles to list
    let mut titles : Vec<String> = Vec::new();
    if !pdf_metadata.metadata_title.is_empty(){
        titles.push(pdf_metadata.metadata_title.to_string());
    }
    if !pdf_metadata.assumed_title.is_empty(){
        titles.push(pdf_metadata.assumed_title.to_string());
    }
    
    // to store top result
    let mut top_result : Option<Metadata> = None;

    for title in titles{
        // Extarct title and encode title
        let title_raw = title.trim();
        let title_query = encode(&title.trim());
        
        let binding = "".to_string();
        // Collect first author or set it to empry string if empty
        let author_raw = pdf_metadata.author.get(0).unwrap_or(&binding).trim();
    
        // Only encode author_raw if it is not "N/A" or empty
        let author_query = if author_raw == "N/A" || author_raw.is_empty() {
            "".to_string()
        } else {
            encode(author_raw).into_owned()
        };

        // Construct the request URL
        let request_url = if author_query.is_empty() {
            format!("https://api.crossref.org/works?query.bibliographic={}", title_query)
        } else {
            format!(
                "https://api.crossref.org/works?query.bibliographic={}&query.author={}",
                title_query, author_query
            )
        };
    
        // Print URL for requset (Can be removed if print not wanted)
        // println!("🔍 API Request URL: {}", request_url);
    
        let json = fetch_with_retry(&request_url).await?;
    
        // Check status of the response return error if not ok
        if json["status"] != "ok" {
            eprintln!(" Crossref API returned an error: {:?}", json);
            return Err("Crossref API Error".into());
        }
    
        // Check if any results are returned, if not return error
        let total_results = json["message"]["total-results"].as_i64().unwrap_or(0);
        if total_results == 0 {
            return Err("No metadata found.".into());
        }
    
        // Create vector to collect all result
        let mut metadata_list = Vec::new();
        
        if let Some(items) = json["message"]["items"].as_array() {
            for work in items {
                let result_title = extract_array_str(work, "title");
                let title_confidence = compare_results(&result_title, title_raw);
        
                // Prints that can be used to easy compare tittles
                // println!("Found Title: {}. With: {}", result_title, title_raw);
                // println!("Title Confidence: {:.2}%", title_confidence);
                // println!("------------------------------");
    
                let metadata = Metadata {
                    title: result_title,
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
                    title_confidence: title_confidence,
                    file_path : pdf_metadata.path.clone(),
                };
    
                metadata_list.push(metadata);
            }
        }
    
        // Sort metadata by title confidence. Save the best
        metadata_list.sort_by(|a, b| b.title_confidence.partial_cmp(&a.title_confidence).unwrap_or(std::cmp::Ordering::Equal));
        let Some(highscore) = metadata_list.get(0) else{
            continue;
        };

        if let Some(ref top) = top_result{
            if top.title_confidence < highscore.title_confidence{
                top_result = Some(highscore.clone());
            }
        }
        else{
            top_result = Some(highscore.clone());
        }
    }

    Ok(top_result)
}