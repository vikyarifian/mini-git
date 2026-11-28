use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub hash: String,
    pub path: String,
}

impl IndexEntry {
    pub fn new(hash: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            path: path.into(),
        }
    }

    pub fn parse_line(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            Some(Self {
                hash: parts[0].to_string(),
                path: parts[1].to_string(),
            })
        } else {
            None
        }
    }

    pub fn to_line(&self) -> String {
        format!("{} {}", self.hash, self.path)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IndexManifest {
    pub entries: BTreeMap<String, String>,
}

impl IndexManifest {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn load(git_dir: &Path) -> io::Result<Self> {
        let index_path = git_dir.join("index");
        if !index_path.exists() {
            return Ok(Self::new());
        }

        let file = fs::File::open(&index_path)?;
        let reader = BufReader::new(file);
        let mut entries = BTreeMap::new();

        for line in reader.lines() {
            let line = line?;
            // std::io limitation: BufRead::lines strips newline but not carriage returns on windows, normalizing string manually
            let line = line.trim_end_matches('\r');
            if let Some(entry) = IndexEntry::parse_line(line) {
                entries.insert(entry.path, entry.hash);
            }
        }

        Ok(Self { entries })
    }

    pub fn save(&self, git_dir: &Path) -> io::Result<()> {
        let index_path = git_dir.join("index");
        let content = self.serialize();
        fs::write(index_path, content)?;
        Ok(())
    }

    pub fn add(&mut self, path: String, hash: String) {
        self.entries.insert(path, hash);
    }

    pub fn remove(&mut self, path: &str) -> bool {
        self.entries.remove(path).is_some()
    }

    pub fn get(&self, path: &str) -> Option<&String> {
        self.entries.get(path)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn serialize(&self) -> String {
        let mut lines = Vec::new();
        for (path, hash) in &self.entries {
            lines.push(format!("{} {}", hash, path));
        }
        if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        }
    }

    pub fn parse(content: &str) -> Self {
        let mut entries = BTreeMap::new();
        for line in content.lines() {
            if let Some(entry) = IndexEntry::parse_line(line) {
                entries.insert(entry.path, entry.hash);
            }
        }
        Self { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_entry_parse() {
        let line = "da39a3ee5e6b4b0d3255bfef95601890afd80709 test/file.txt";
        let entry = IndexEntry::parse_line(line).unwrap();
        assert_eq!(entry.hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(entry.path, "test/file.txt");
    }
}
