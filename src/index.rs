use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use scraper::{Html, Selector};
use reqwest::{header, Error, Response};
use crate::disk;
use crate::chain;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::task;
use anyhow::anyhow;
use futures::future::try_join_all;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};
use futures::executor::block_on;


#[derive(Serialize, Deserialize, Debug, Eq, Hash, PartialEq, Clone)]
pub(crate) struct WordLocation {
    ens_name: String,
    pub(crate) ipfs_hash: String,
    pub(crate) location: usize
}

// index = string -> [word_locations]
pub(crate) type Index = HashMap<String, HashSet<WordLocation>>;
// docs = ipfs_hash -> [words_in_doc]
pub(crate) type Docs = HashMap<String, Vec<String>>;

const MAX_CONCURRENT_REQUESTS: usize = 10;
const MAX_RETRIES: usize = 5;

fn get_delay(i: usize) -> Duration {
    let sleep_duration = (1 + 2_i32.pow(i as u32));
    Duration::from_secs(sleep_duration as u64)
}

async fn make_request(url: &str) -> Result<Response, anyhow::Error> {
    for i in 0..MAX_RETRIES {
        let response = reqwest::get(url).await?;
        let result = match response.status() {
            reqwest::StatusCode::OK => Ok(response),
            reqwest::StatusCode::BAD_REQUEST => Err(anyhow!("Bad Request")),
            reqwest::StatusCode::UNAUTHORIZED => Err(anyhow!("Unauthorized")),
            reqwest::StatusCode::FORBIDDEN => Err(anyhow!("Forbidden")),
            reqwest::StatusCode::NOT_FOUND => Err(anyhow!("Not Found")),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR => Err(anyhow!("Internal Server Error")),
            reqwest::StatusCode::TOO_MANY_REQUESTS => Err(anyhow!("Too Many Requests")),
            status => Err(anyhow!("Unexpected status code: {}", status))
        };

        if let Err(e) = &result {
            let error_message = e.to_string();
            if error_message == "Too Many Requests" {
                let delay = get_delay(i);
                println!("Rate limited. Retrying in {} seconds", delay.as_secs());
                sleep(delay).await;
                continue;
            } else {
                println!("Error: {}", error_message);
            }
        } else {
            println!("Success! {}", url);
        }

        return result;
    }

    return Err(anyhow!("Failed to retrieve {} after {} retries", url, MAX_RETRIES));
}

async fn process_response(response: Response, ipfs_hash: String) -> Result<(Index, Docs), Error> {
    let mut index = Index::new();
    let mut docs = Docs::new();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();  // Clone the string here

    let body = response.text().await?;

    match content_type {
        // Check if the Content-Type is HTML
        ct if ct.contains("text/html") => {
            let document = Html::parse_document(&body);
            // todo: include head + body?
            let selector = Selector::parse("body").unwrap();
            let elements = document.select(&selector);
            for element in elements {
                let words = element.text().map(|s| s.to_owned()).collect::<Vec<_>>();

//                let words = element.text().collect::<Vec<_>>();
                let text = words.join(" ");
                docs.insert(ipfs_hash.clone(), words);
                process_text(&text, ipfs_hash.clone(), &mut index);
            }
        },
        // Check if the Content-Type is plain text
        ct if ct.contains("text/plain") || ct.contains("application/json") => {
            process_text(&body, ipfs_hash.clone(), &mut index);
        },
        // Handle other content types, including binary
        _ => {
            println!("The document is not HTML or plain text or json. Content type: {}", content_type);
            // Here, you might want to log this event, or handle binary data if needed.
        },
    }

    Ok((index, docs))
}

fn process_text(text: &str, ipfs: String, index: &mut Index) {
    text.split_whitespace()
        .enumerate()
        .for_each(|(i, word)| {
            let loc = WordLocation {
                ens_name: String::from("???"),
                ipfs_hash: ipfs.clone(),
                location: i,
            };
            index.entry(String::from(word))
                .or_insert_with(HashSet::new)
                .insert(loc);
        });
}

fn print_index(index: &Index) {
    // Iterate over all values in all vectors associated with every key
    for (key, values) in index.iter() {
        println!("Values for key '{}':", key);
        for value in values {
            println!("- {} - {}", value.ens_name, value.location);
        }
    }
}

async fn build_index(index_path: &Path, docs_path: &Path) -> (Index, Docs) {
    println!("Building index");
    let index = Arc::new(Mutex::new(Index::new()));
    let docs = Arc::new(Mutex::new(HashMap::new()));
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));

    let hashes = chain::get_events().await.unwrap();
    let futures: Vec<_> = hashes.iter().enumerate().map(|(i, hash)| {
        let semaphore = Arc::clone(&semaphore);
        let index = Arc::clone(&index);
        let docs = Arc::clone(&docs);
        let hash = hash.clone();
        // let url = format!("https://gateway.pinata.cloud/ipfs/{}", hash);
        let url = format!("https://ipfs.io/ipfs/{}", hash);
        println!("Making request to url {}/{} : {}", i + 1, hashes.len(), url);
        task::spawn(async move {
            // Acquire a permit from the semaphore
            let permit = block_on(semaphore.acquire()).unwrap();

            let result = make_request(&url).await;
            match result {
                Ok(response) => {
                    let result = process_response(response, hash).await;
                    match result {
                        Ok((local_index, local_docs)) => {
                            let mut locked_index = index.lock().unwrap();
                            for (key, value) in local_index {
                                locked_index.entry(key)
                                    .and_modify(|e| e.extend(value.clone()))
                                    .or_insert(value);
                            }
                            docs.lock().unwrap().extend(local_docs);
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
            drop(permit);
        })
    }).collect();

    try_join_all(futures).await.expect("Could not join futures together");

    print_index(&index.lock().unwrap());
    disk::save_index(index_path, &index.lock().unwrap()).expect("Could not save index to file");
    disk::save_docs(docs_path, &docs.lock().unwrap()).expect("Could not save docs to file");
    let index = index.lock().unwrap().clone();
    let docs = docs.lock().unwrap().clone();

    return (index, docs);
}

pub async fn load_index(force_rebuild: bool) -> (Index, Docs) {
    // Save index to disk
    let index_path = Path::new("index.json");
    let docs_path = Path::new("docs.json");
    if index_path.exists() && docs_path.exists() && !force_rebuild {
        let index = disk::load_index(index_path).expect("Could not load index from file");
        let docs = disk::load_docs(docs_path).expect("Could not load docs from file");
        return (index, docs);
    } else {
        return build_index(index_path, docs_path).await;
    }
}