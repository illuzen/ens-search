use std::collections::{HashMap, HashSet};
use std::io::{stdin, stdout, Write};
use tokio;
use ethers::prelude::*;
use regex::Regex;
use serde::{Serialize, Deserialize};
use tokio::io::split;
use warp::{http::Response, Filter};
use crate::index::Docs;

mod index;
mod chain;
mod disk;

#[derive(PartialEq)]
enum QueryToken {
    Word(String),
    Phrase(String),
    And,
    Or,
}

struct Query {
    subquery1: Option<Box<Query>>,
    subquery2: Option<Box<Query>>,
    connector: Option<QueryToken>,
    base: Option<String>
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
struct QueryResponse {
    ens_name: String,
    ipfs_hash: String,
    context: String
}

const CONTEXT_WINDOW: usize = 5;
const MAX_RESULTS: i32 = 20;

fn retrieve_from_index(query: Query, index: &index::Index, docs: &Docs) -> Vec<QueryResponse> {
    let mut results: Vec<QueryResponse> = Vec::new();

    if let Some(base) = query.base {
        let locs = index.get(&base).unwrap();
        for loc in locs {
            let doc = docs.get(&loc.ipfs_hash).unwrap();
            let doc_slice = &doc[loc.location-CONTEXT_WINDOW..loc.location+CONTEXT_WINDOW];
            results.push(QueryResponse {
                ens_name: "???".to_string(),
                ipfs_hash: loc.ipfs_hash.clone(),
                context: doc_slice.join(" "),
            })
        }
    } else if query.connector == Some(QueryToken::Or) {
        let left = retrieve_from_index(*query.subquery1.unwrap(), index, docs);
        let right = retrieve_from_index(*query.subquery2.unwrap(), index, docs);
        let left_set: HashSet<_> = left.into_iter().collect();
        let right_set: HashSet<_> = right.into_iter().collect();

        return left_set.union(&right_set).cloned().collect();
    } else if query.connector == Some(QueryToken::And) {
        let left = retrieve_from_index(*query.subquery1.unwrap(), index, docs);
        let right = retrieve_from_index(*query.subquery2.unwrap(), index, docs);
        let left_set: HashSet<_> = left.into_iter().collect();
        let right_set: HashSet<_> = right.into_iter().collect();

        return left_set.intersection(&right_set).cloned().collect();
    }

    return results;
}

// async fn handle_query(query: Query, index: &index::Index) -> Result<impl warp::Reply, warp::Rejection> {
//     let results = vec![
//         QueryResponse {
//             ens_name: "dummy.eth".parse().unwrap(),
//             ipfs_hash: "QmbQ7z3CftrbvKYXsA4iDkjWVUB5nX2tRJdJgYYWJTJfja".parse().unwrap(),
//             context: "context".parse().unwrap()
//         },
//         QueryResponse {
//             ens_name: "dummy2.eth".parse().unwrap(),
//             ipfs_hash: "QmbQ7z3CftrbvKYXsA4iDkjWVUB5nX2tRJdJgYYWJTJfja".parse().unwrap(),
//             context: "context2".parse().unwrap()
//         }
//     ];
//
//     // Convert the results to JSON and wrap it in a Result
//     let json_reply = warp::reply::json(&results);
//     Ok(json_reply)
// }

fn parse_query(query_str: String) -> Query {
    let words: Vec<_> = query_str.split(" ").collect();
    if words.len() == 1 {
        // base case single word
        return Query { subquery1: None, subquery2: None, connector: None, base: Some(query_str) }
    } else if query_str.starts_with('"') && query_str.ends_with('"') {
        // exact match base case
        return Query { subquery1: None, subquery2: None, connector: None, base: Some(query_str) }
    }

    for (i, word) in words.iter().enumerate() {
        if word.to_uppercase().as_str() == "AND" && i > 0 && i < words.len() - 1 {
            return Query {
                subquery1: Some(Box::new(parse_query(words[..i].join(" ")))),
                subquery2: Some(Box::new(parse_query(words[i + 1..].join(" ")))),
                connector: Some(QueryToken::And),
                base: None
            }
        }
    }

    for (i, word) in words.iter().enumerate() {
        if word.to_uppercase().as_str() == "OR" && i > 0 && i < words.len() - 1 {
            return Query {
                subquery1: Some(Box::new(parse_query(words[..i].join(" ")))),
                subquery2: Some(Box::new(parse_query(words[i + 1..].join(" ")))),
                connector: Some(QueryToken::Or),
                base: None
            }
        }
    }

    return Query {
        subquery1: Some(Box::new(parse_query(words[0].to_string()))),
        subquery2: Some(Box::new(parse_query(words[1..].join(" ")))),
        connector: Some(QueryToken::Or),
        base: None
    }
}

fn tokenize_query(query: String) -> Vec<QueryToken> {
    let mut tokens = Vec::new();
    let re = Regex::new(r#""[^"]*"|\S+"#).unwrap();

    for token in re.find_iter(&*query) {
        let mut token = token.as_str();
        if token.starts_with('"') && token.ends_with('"') {
            // Remove the surrounding quotes from phrases
            token = &token[1..token.len() - 1];
            tokens.push(QueryToken::Phrase(token.to_string()));
        } else {
            match token.to_uppercase().as_str() {
                "AND" => tokens.push(QueryToken::And),
                "OR" => tokens.push(QueryToken::Or),
                _ => tokens.push(QueryToken::Word(token.to_string())),
            }
        }
    }

    tokens
}

// fn process_query(query: String, index: &index::Index) -> Vec<QueryResponse> {
//     let tokens = tokenize_query(query);
//     for token in tokens {
//         match token {
//             QueryToken::Word(word) => println!("Word: {}", word),
//             QueryToken::Phrase(phrase) => println!("Phrase: {}", phrase),
//             QueryToken::And => println!("AND"),
//             QueryToken::Or => println!("OR"),
//         }
//     }
//     vec![
//         QueryResponse { ens_name: "dummy.eth".to_string(), ipfs_hash: "QmbQ7z3CftrbvKYXsA4iDkjWVUB5nX2tRJdJgYYWJTJfja".to_string(), context: "context".to_string() },
//         QueryResponse { ens_name: "dummy.eth".to_string(), ipfs_hash: "QmbQ7z3CftrbvKYXsA4iDkjWVUB5nX2tRJdJgYYWJTJfja".to_string(), context: "context".to_string() },
//     ]
// }

fn receive_search(index: &index::Index, docs: &Docs) {
    while true {
        let mut input = String::new();
        print!("Please enter something: ");
        stdout().flush().unwrap(); // Make sure the prompt is immediately displayed
        stdin().read_line(&mut input).unwrap();
        let query = parse_query(input.clone());
        retrieve_from_index(query, index, docs);
        println!("You entered: {}", input.trim());
    }
}

#[tokio::main]
async fn main() {
    let (index, docs) = index::load_index(true).await;
    // let index = HashMap::new();
    println!("Loaded index with {} entries", index.len());

    receive_search(&index, &docs);
//     let query_route = warp::post()
//         .and(warp::path("query"))
//         .and(warp::body::json::<Query>())
//         .and_then(handle_query);
//
//     // Start the server
//     warp::serve(query_route)
//         .run(([127, 0, 0, 1], 3030))
//         .await;
}

