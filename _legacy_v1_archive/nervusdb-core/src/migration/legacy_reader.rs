//! Reader for legacy .synapsedb file format (v1.x)
//!
//! File structure:
//! - Magic header: "SYNAPSEDB" (9 bytes)
//! - Version: u32 (4 bytes)
//! - File header: 64 bytes total
//! - Sections: dictionary, triples, indexes, properties

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::{Result, Triple};

const MAGIC_HEADER: &[u8] = b"SYNAPSEDB";
const FILE_VERSION: u32 = 2;
const FILE_HEADER_LENGTH: u64 = 64;

#[derive(Debug, Clone)]
struct SectionPointer {
    offset: u32,
    length: u32,
}

#[derive(Debug)]
struct FileLayout {
    dictionary: SectionPointer,
    triples: SectionPointer,
    #[allow(dead_code)] // Preserved for file format compatibility
    indexes: SectionPointer,
    properties: SectionPointer,
}

/// Legacy database reader for .synapsedb format
pub struct LegacyDatabase {
    file: File,
    layout: FileLayout,
    dictionary: Vec<String>,
}

impl LegacyDatabase {
    /// Open a legacy .synapsedb file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;

        // Read and validate header
        let layout = Self::read_header(&mut file)?;

        // Read dictionary section
        let dictionary = Self::read_dictionary(&mut file, &layout.dictionary)?;

        Ok(Self {
            file,
            layout,
            dictionary,
        })
    }

    fn read_header(file: &mut File) -> Result<FileLayout> {
        let mut header = vec![0u8; FILE_HEADER_LENGTH as usize];
        file.read_exact(&mut header)?;

        // Validate magic header
        if &header[..MAGIC_HEADER.len()] != MAGIC_HEADER {
            return Err(crate::Error::Other(
                "Invalid .synapsedb file: magic header mismatch".into(),
            ));
        }

        // Validate version
        let version = u32::from_le_bytes([
            header[MAGIC_HEADER.len()],
            header[MAGIC_HEADER.len() + 1],
            header[MAGIC_HEADER.len() + 2],
            header[MAGIC_HEADER.len() + 3],
        ]);

        if version != FILE_VERSION {
            return Err(crate::Error::Other(format!(
                "Unsupported file version: {} (expected {})",
                version, FILE_VERSION
            )));
        }

        // Read section pointers (starting at byte 16)
        let read_section = |offset: usize| -> SectionPointer {
            let base = 16 + offset * 8;
            SectionPointer {
                offset: u32::from_le_bytes([
                    header[base],
                    header[base + 1],
                    header[base + 2],
                    header[base + 3],
                ]),
                length: u32::from_le_bytes([
                    header[base + 4],
                    header[base + 5],
                    header[base + 6],
                    header[base + 7],
                ]),
            }
        };

        Ok(FileLayout {
            dictionary: read_section(0),
            triples: read_section(1),
            indexes: read_section(2),
            properties: read_section(3),
        })
    }

    fn read_dictionary(file: &mut File, section: &SectionPointer) -> Result<Vec<String>> {
        if section.length == 0 {
            return Ok(Vec::new());
        }

        file.seek(SeekFrom::Start(section.offset as u64))?;

        let mut data = vec![0u8; section.length as usize];
        file.read_exact(&mut data)?;

        // First 4 bytes: count
        if data.len() < 4 {
            return Err(crate::Error::Other("Invalid dictionary section".into()));
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut dictionary = Vec::with_capacity(count);

        let mut pos = 4;
        for _ in 0..count {
            if pos + 4 > data.len() {
                return Err(crate::Error::Other("Truncated dictionary entry".into()));
            }

            // Read string length
            let str_len =
                u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;
            pos += 4;

            if pos + str_len > data.len() {
                return Err(crate::Error::Other("Truncated dictionary string".into()));
            }

            // Read string bytes
            let s = String::from_utf8(data[pos..pos + str_len].to_vec())
                .map_err(|e| crate::Error::Other(format!("Invalid UTF-8 in dictionary: {}", e)))?;

            dictionary.push(s);
            pos += str_len;
        }

        Ok(dictionary)
    }

    /// Get the dictionary (string ID -> string mapping)
    pub fn dictionary(&self) -> &[String] {
        &self.dictionary
    }

    /// Iterate over all triples in the legacy database
    pub fn read_triples(&mut self) -> Result<Vec<Triple>> {
        if self.layout.triples.length == 0 {
            return Ok(Vec::new());
        }

        self.file
            .seek(SeekFrom::Start(self.layout.triples.offset as u64))?;

        let mut data = vec![0u8; self.layout.triples.length as usize];
        self.file.read_exact(&mut data)?;

        // First 4 bytes: count
        if data.len() < 4 {
            return Err(crate::Error::Other("Invalid triples section".into()));
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut triples = Vec::with_capacity(count);

        let mut pos = 4;
        for _ in 0..count {
            if pos + 24 > data.len() {
                // Each triple is 3 * u64 = 24 bytes
                return Err(crate::Error::Other("Truncated triple entry".into()));
            }

            let subject_id = u64::from_le_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]);
            let predicate_id = u64::from_le_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]);
            let object_id = u64::from_le_bytes([
                data[pos + 16],
                data[pos + 17],
                data[pos + 18],
                data[pos + 19],
                data[pos + 20],
                data[pos + 21],
                data[pos + 22],
                data[pos + 23],
            ]);

            triples.push(Triple {
                subject_id,
                predicate_id,
                object_id,
            });

            pos += 24;
        }

        Ok(triples)
    }

    /// Read properties section (JSON string format)
    pub fn read_properties(&mut self) -> Result<Vec<u8>> {
        if self.layout.properties.length == 0 {
            return Ok(Vec::new());
        }

        self.file
            .seek(SeekFrom::Start(self.layout.properties.offset as u64))?;

        let mut data = vec![0u8; self.layout.properties.length as usize];
        self.file.read_exact(&mut data)?;

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_synapsedb() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();

        // Write header
        let mut header = vec![0u8; FILE_HEADER_LENGTH as usize];
        header[..MAGIC_HEADER.len()].copy_from_slice(MAGIC_HEADER);
        header[MAGIC_HEADER.len()..MAGIC_HEADER.len() + 4]
            .copy_from_slice(&FILE_VERSION.to_le_bytes());

        // Empty sections
        file.write_all(&header).unwrap();
        file.flush().unwrap();

        file
    }

    #[test]
    fn test_open_legacy_database() {
        let file = create_test_synapsedb();
        let result = LegacyDatabase::open(file.path());
        assert!(result.is_ok());
    }
}
