use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectType {
    Blob,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobObject {
    pub content: Vec<u8>,
}

impl BlobObject {
    pub fn new(content: Vec<u8>) -> Self {
        Self { content }
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let content = fs::read(path)?;
        Ok(Self { content })
    }

    pub fn hash(&self) -> String {
        sha1(&self.content)
    }

    pub fn save(&self, git_dir: &Path) -> io::Result<String> {
        let hash = self.hash();
        let object_path = git_dir.join("objects").join(&hash);
        if !object_path.exists() {
            fs::write(object_path, &self.content)?;
        }
        Ok(hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitObject {
    pub author: String,
    pub timestamp: u64,
    pub parent: String,
    pub index_manifest: String,
    pub message: String,
}

impl CommitObject {
    pub fn new(
        author: String,
        timestamp: u64,
        parent: String,
        index_manifest: String,
        message: String,
    ) -> Self {
        Self {
            author,
            timestamp,
            parent,
            index_manifest,
            message,
        }
    }

    pub fn serialize(&self) -> String {
        let parent_str = if self.parent.trim().is_empty() {
            "none"
        } else {
            self.parent.trim()
        };
        format!(
            "author {}\ntimestamp {}\nparent {}\n\n{}\n---\n{}",
            self.author,
            self.timestamp,
            parent_str,
            self.index_manifest.trim(),
            self.message
        )
    }

    pub fn hash(&self) -> String {
        sha1(self.serialize().as_bytes())
    }

    pub fn save(&self, git_dir: &Path) -> io::Result<String> {
        let payload = self.serialize();
        let hash = sha1(payload.as_bytes());
        let object_path = git_dir.join("objects").join(&hash);
        fs::write(object_path, payload)?;
        Ok(hash)
    }

    pub fn load(git_dir: &Path, hash: &str) -> io::Result<Self> {
        let object_path = git_dir.join("objects").join(hash);
        let content = fs::read_to_string(object_path)?;
        Self::parse(&content).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Failed to parse commit object")
        })
    }

    pub fn parse(content: &str) -> Option<Self> {
        let parts: Vec<&str> = content.splitn(2, "\n---\n").collect();
        if parts.len() < 2 {
            return None;
        }

        let message = parts[1].to_string();
        let header_and_index = parts[0];

        let mut author = String::new();
        let mut timestamp = 0u64;
        let mut parent = String::new();
        let mut lines = header_and_index.lines();
        let mut body_started = false;
        let mut index_lines = Vec::new();

        while let Some(line) = lines.next() {
            if body_started {
                index_lines.push(line);
            } else if line.is_empty() {
                body_started = true;
            } else if line.starts_with("author ") {
                author = line["author ".len()..].to_string();
            } else if line.starts_with("timestamp ") {
                timestamp = line["timestamp ".len()..].parse().unwrap_or(0);
            } else if line.starts_with("parent ") {
                parent = line["parent ".len()..].to_string();
            }
        }

        // std::path limitation: PathBuf does not normalize relative path separators automatically, using string replacement for index keys
        let index_manifest = index_lines.join("\n").replace("\\", "/");

        Some(Self {
            author,
            timestamp,
            parent,
            index_manifest,
            message,
        })
    }
}

pub fn sha1(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() * 8) % 512 != 448 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                60..=79 => (b ^ c ^ d, 0xCA62C1D6),
                _ => unreachable!(),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_object_hash() {
        let blob = BlobObject::new(b"hello world".to_vec());
        assert_eq!(blob.hash(), "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
    }
}
