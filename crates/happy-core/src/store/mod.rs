use std::fs;
use std::io;
use std::path::Path;

use crate::vector::BM25Index;

/// Version header for serialized data format.
const FORMAT_VERSION: u32 = 1;

/// Header written at the start of serialized files.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoreHeader {
    version: u32,
    kind: String,
}

/// Save a BM25 index to disk.
pub fn save_bm25(index: &BM25Index, path: &Path) -> io::Result<()> {
    let header = StoreHeader {
        version: FORMAT_VERSION,
        kind: "bm25".to_string(),
    };

    let header_bytes =
        bincode::serialize(&header).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let data_bytes =
        bincode::serialize(index).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let mut output = Vec::new();
    output.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(&header_bytes);
    output.extend_from_slice(&data_bytes);

    // Atomic write: write to temp file, then rename
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &output)?;
    fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load a BM25 index from disk.
pub fn load_bm25(path: &Path) -> io::Result<BM25Index> {
    let data = fs::read(path)?;

    if data.len() < 4 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too small"));
    }

    let header_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + header_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated header",
        ));
    }

    let header: StoreHeader = bincode::deserialize(&data[4..4 + header_len])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if header.version != FORMAT_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported format version: {}", header.version),
        ));
    }

    let index: BM25Index = bincode::deserialize(&data[4 + header_len..])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(index)
}

/// Save code elements to disk.
pub fn save_elements(elements: &[crate::indexer::CodeElement], path: &Path) -> io::Result<()> {
    let header = StoreHeader {
        version: FORMAT_VERSION,
        kind: "elements".to_string(),
    };

    let header_bytes =
        bincode::serialize(&header).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let data_bytes =
        bincode::serialize(elements).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let mut output = Vec::new();
    output.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(&header_bytes);
    output.extend_from_slice(&data_bytes);

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &output)?;
    fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load code elements from disk.
pub fn load_elements(path: &Path) -> io::Result<Vec<crate::indexer::CodeElement>> {
    let data = fs::read(path)?;

    if data.len() < 4 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too small"));
    }

    let header_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + header_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated header",
        ));
    }

    let _header: StoreHeader = bincode::deserialize(&data[4..4 + header_len])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let elements: Vec<crate::indexer::CodeElement> = bincode::deserialize(&data[4 + header_len..])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_roundtrip() {
        let mut index = BM25Index::new();
        index.add_document("doc1", "hello world");
        index.add_document("doc2", "foo bar");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bm25.bin");

        save_bm25(&index, &path).unwrap();
        let loaded = load_bm25(&path).unwrap();

        assert_eq!(loaded.len(), 2);
        let results = loaded.search("hello", 5);
        assert!(!results.is_empty());
    }
}
