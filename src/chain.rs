use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::str::{from_utf8, FromStr};
use ethers::prelude::*;
use ethers::providers::{Provider, Http};
use std::sync::Arc;
use ethers::utils::hex;
use ethers::types::{H256};
use substring::Substring;
use bs58;
use cid::Cid;
use crate::disk;

// use ethers::utils::hex::hex;
// use sha3::{Digest, Keccak256};
//
// fn keccak(input: &str) -> &str {
//     // Create a Keccak256 object
//     let mut hasher = Keccak256::new();
//
//     // Write input message
//     hasher.update(input);
//
//     // Read hash digest and consume hasher
//     let result = hasher.finalize();
//
//     // Convert hash to hex string
//     return hex::encode(result);
// }

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

fn decode_cidv0(hex_string: &str) -> String { //Result<Vec<u8>, bs58::decode::Error> {
    let bytes = hex::decode(hex_string).expect("Invalid hex string");
    // Assuming the bytes are directly the raw data of the CID (you might need to adjust this)
    // Create a CID instance from the bytes
    let cid = Cid::try_from(bytes).expect("Failed to parse CID");

    // Encode the CID's multihash to Base58
    let ipfs_hash = cid.to_string();
    return ipfs_hash;
    // return bs58::decode(hex_string).into_vec();
}

pub async fn get_events() -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let path = "cids.csv";

    if let Ok(lines) = disk::load_events(path) {
        println!("Loaded events from disk successfully");
        return Ok(lines);
    } else {
        println!("Getting events from network");
    }

    // Connect to the Ethereum network
    let provider = Provider::<Http>::try_from("https://mainnet.infura.io/v3/eb4d26a77ba04ab2ba6bf10a6a86f120")?;
    let provider = Arc::new(provider);
    println!("Connected to ethereum provider infura");

    // Get the latest block number
    let to_block = provider.get_block_number().await?;

    // Define the event signature for "SetContentHash()"
    // This hash is typically the Keccak-256 hash of the event signature string
    // let event_sig = keccak("ContenthashChanged(bytes32,bytes)");
    let event_sig = "e379c1624ed7e714cc0937528a32359d69d5281337765313dba4e081b72d7578";
    let ens_contract = "0x231b0ee14048e9dccd1d247744d114a4eb5e8e63";
    // let ens_contract = "0x4976fb03C32e5B8cfe2b6cCB31c09Ba78EBaBa41"; // other resolver
    let ens_contract_address = ens_contract.parse::<H160>()
        .map_err(|_| "Failed to parse contract address")?;
    let event_hash = H256::from_str(event_sig)
        .map_err(|_| "Failed to parse event signature hash")?;

    // let event_signature_hash = H256::from_slice(&hex::decode("e379c1624ed7e714cc0937528a32359d69d5281337765313dba4e081b72d7578")?);
    // let from_block = to_block - 100000;
    let from_block = 0;

    println!("Filtering for events with signature {} from block {} to {}", event_sig, from_block, to_block);
    // Create a filter for the event

    let filter = Filter::new()
        .address(ens_contract_address)
        // .event(event_sig)
        .from_block(from_block)
        .to_block(to_block)
        .topic0(event_hash);


    // Query the filter logs
    let logs = provider.get_logs(&filter).await?;

    println!("Acquired {} logs", logs.len());
    let mut ipfs_hashes = HashSet::new();

    // Process the logs
    for log in logs {
        // TODO: how to get domain from log???
        let d = log.data;
        let h = hex::encode(d);
        let v: Vec<_> = h.split("26e301").collect();
        println!("Log data (original): {}", h);
        let l = v.len();
        match l {
            2 => {
                let bytestring = v[1].substring(0, 72);
                let ipfs = decode_cidv0(bytestring);
                println!("Ipfs: {} - {:?}", bytestring, ipfs);
                ipfs_hashes.insert(ipfs);
            },
            1 => println!("Unknown: {}", v[0]),
            _ => println!("bad vector")
        }
    }

    disk::save_events(path, ipfs_hashes.clone());

    Ok(ipfs_hashes)
}