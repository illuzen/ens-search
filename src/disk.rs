use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use crate::index;
use std::io::{self, BufRead, Write, Error};


pub fn save_docs(filename: &Path, docs: &index::Docs) -> io::Result<()> {
    // Open a file in write mode
    let file = File::create(filename)?;
    let writer = io::BufWriter::new(file);

    // Serialize the HashMap into JSON and write it to the file
    serde_json::to_writer(writer, &docs).map_err(|e| Error::new(io::ErrorKind::Other, e))
}

pub fn load_docs(filename: &Path) -> io::Result<index::Docs> {
    println!("Loading docs from file: {:?}", filename.to_str());
    // Open the file in read mode
    let file = File::open(filename)?;
    let reader = io::BufReader::new(file);

    // Deserialize the JSON from the file back into a HashMap
    serde_json::from_reader(reader).map_err(|e| Error::new(io::ErrorKind::Other, e))
}

pub fn save_index(filename: &Path, index: &index::Index) -> io::Result<()> {
    // Open a file in write mode
    let file = File::create(filename)?;
    let writer = io::BufWriter::new(file);

    // Serialize the HashMap into JSON and write it to the file
    serde_json::to_writer(writer, &index).map_err(|e| Error::new(io::ErrorKind::Other, e))
}

pub fn load_index(filename: &Path) -> io::Result<index::Index> {
    println!("Loading index from file: {:?}", filename.to_str());
    // Open the file in read mode
    let file = File::open(filename)?;
    let reader = io::BufReader::new(file);

    // Deserialize the JSON from the file back into a HashMap
    serde_json::from_reader(reader).map_err(|e| Error::new(io::ErrorKind::Other, e))
}


pub fn load_events(filename: &str) -> Result<HashSet<String>, Error> {
    let path = Path::new(filename);
    if path.exists() {
        let file = File::open(path)?;
        let reader = io::BufReader::new(file);
        // Create a HashSet to store the lines
        let mut lines = HashSet::new();

        // Read lines using the lines iterator from BufReader
        for line_result in reader.lines() {
            let line = line_result?;  // Handle potential I/O errors
            lines.insert(line);       // Insert each line into the HashSet
        }

        Ok(lines)
    } else {
        println!("Could not find {}", filename);
        Err(Error::new(io::ErrorKind::Other, "Could not find file for events"))
    }
}

pub fn save_events(path: &str, events: HashSet<String>) -> Result<(),Error> {
    // Open a file in write-only mode, returns `io::Result<File>`
    let mut file = File::create(path)?;

    // Iterate over each line in the vector
    for line in events {
        // Write the line to the file and add a newline character
        file.write_fmt(format_args!("{}\n", line)).expect("Unable to write to file");
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

