mod index;
mod object;
mod repository;

use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn sha1(data: &[u8]) -> String {
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

            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
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

fn get_git_dir() -> io::Result<PathBuf> {
    let mut curr = env::current_dir()?;
    loop {
        let candidate = curr.join(".minigit");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !curr.pop() {
            break;
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "not a mini-git repository (or any of the parent directories): .minigit",
    ))
}

fn cmd_init(target_dir: Option<&str>) -> io::Result<()> {
    let base_path = match target_dir {
        Some(dir) => PathBuf::from(dir),
        None => env::current_dir()?,
    };

    let git_dir = base_path.join(".minigit");
    let objects_dir = git_dir.join("objects");

    fs::create_dir_all(&objects_dir)?;

    let index_file = git_dir.join("index");
    if !index_file.exists() {
        File::create(&index_file)?;
    }

    let head_file = git_dir.join("HEAD");
    if !head_file.exists() {
        File::create(&head_file)?;
    }

    println!("Initialized empty mini-git repository in {}", git_dir.display());
    Ok(())
}

fn cmd_add(file_path_str: &str) -> io::Result<()> {
    let git_dir = get_git_dir()?;
    let path = Path::new(file_path_str);
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("pathspec '{}' did not match any files", file_path_str),
        ));
    }

    let content = fs::read(path)?;
    let hash = sha1(&content);

    let object_path = git_dir.join("objects").join(&hash);
    fs::write(object_path, content)?;

    let index_path = git_dir.join("index");
    let index_content = fs::read_to_string(&index_path).unwrap_or_default();

    let mut new_entries = Vec::new();
    let mut replaced = false;

    for line in index_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            if parts[1] == file_path_str {
                new_entries.push(format!("{} {}", hash, file_path_str));
                replaced = true;
            } else {
                new_entries.push(line.to_string());
            }
        }
    }

    if !replaced {
        new_entries.push(format!("{} {}", hash, file_path_str));
    }

    let new_index_content = new_entries.join("\n") + "\n";
    fs::write(index_path, new_index_content)?;

    println!("Staged {} ({})", file_path_str, &hash[..7]);
    Ok(())
}

fn cmd_commit(message: Option<String>, author: Option<String>) -> io::Result<()> {
    let git_dir = get_git_dir()?;
    let msg = match message {
        Some(m) if !m.is_empty() => m,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Aborting commit due to empty commit message.",
            ));
        }
    };

    let author_name = author.unwrap_or_else(|| "staf_ops".to_string());

    let index_path = git_dir.join("index");
    let index_content = fs::read_to_string(&index_path).unwrap_or_default();
    if index_content.trim().is_empty() {
        println!("nothing to commit, working tree clean");
        return Ok(());
    }

    let head_path = git_dir.join("HEAD");
    let parent_hash = fs::read_to_string(&head_path)
        .unwrap_or_default()
        .trim()
        .to_string();

    let parent_val = if parent_hash.is_empty() {
        "none".to_string()
    } else {
        parent_hash
    };

    // std::time limitation: SystemTime does not support custom ISO-8601 formatting without external crates, using UNIX timestamp seconds instead
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let commit_payload = format!(
        "author {}\ntimestamp {}\nparent {}\n\n{}\n---\n{}",
        author_name, timestamp, parent_val, index_content.trim(), msg
    );

    let commit_hash = sha1(commit_payload.as_bytes());

    let object_path = git_dir.join("objects").join(&commit_hash);
    fs::write(object_path, commit_payload)?;

    fs::write(head_path, &commit_hash)?;

    println!(
        "[{}] {}",
        &commit_hash[..7],
        msg.lines().next().unwrap_or("")
    );
    Ok(())
}

fn cmd_log() -> io::Result<()> {
    let git_dir = get_git_dir()?;
    let head_path = git_dir.join("HEAD");
    let mut current_hash = fs::read_to_string(&head_path)
        .unwrap_or_default()
        .trim()
        .to_string();

    if current_hash.is_empty() {
        println!("No commits yet.");
        return Ok(());
    }

    while !current_hash.is_empty() && current_hash != "none" {
        let object_path = git_dir.join("objects").join(&current_hash);
        if !object_path.exists() {
            break;
        }

        let content = fs::read_to_string(object_path)?;
        let mut author = "unknown";
        let mut timestamp = "0";
        let mut parent = "none";
        let mut message = String::new();

        let mut lines = content.lines();
        while let Some(line) = lines.next() {
            if line.starts_with("author ") {
                author = &line["author ".len()..];
            } else if line.starts_with("timestamp ") {
                timestamp = &line["timestamp ".len()..];
            } else if line.starts_with("parent ") {
                parent = &line["parent ".len()..];
            } else if line == "---" {
                message = lines.collect::<Vec<&str>>().join("\n");
                break;
            }
        }

        println!("commit {}", current_hash);
        println!("Author: {}", author);
        println!("Date:   {}", timestamp);
        println!("\n    {}\n", message.trim());

        current_hash = parent.to_string();
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: mini-git <command> [<args>]");
        println!("\nCommands:");
        println!("   init [dir]                     Initialize repo");
        println!("   add <file>                     Stage file");
        println!("   commit -m <msg> [--author <a>] Create commit");
        println!("   log                            Show history log");
        return;
    }

    let command = &args[1];
    let result = match command.as_str() {
        "init" => {
            let dir = args.get(2).map(|s| s.as_str());
            cmd_init(dir)
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("Error: add requires a file path");
                std::process::exit(1);
            }
            cmd_add(&args[2])
        }
        "commit" => {
            let mut message = None;
            let mut author = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "-m" && i + 1 < args.len() {
                    message = Some(args[i + 1].clone());
                    i += 2;
                } else if args[i] == "--author" && i + 1 < args.len() {
                    author = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cmd_commit(message, author)
        }
        "log" => cmd_log(),
        _ => {
            eprintln!("Unknown command: {}", command);
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_empty() {
        let hash = sha1(b"");
        assert_eq!(hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }
}
