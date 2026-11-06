//! Versioned text fixtures shared by performance probes. Setup is outside timing.
use std::io::Write;
use std::path::Path;

pub const REVISION: u64 = 2;
pub fn kind() -> &'static str {
    if std::env::var("SCRATCHPAD_FIXTURE_KIND").as_deref() == Ok("dedup") {
        "dedup"
    } else {
        "non_repeating"
    }
}

pub fn text(bytes: usize, seed: u64) -> String {
    let mut output = String::with_capacity(bytes);
    let mut state = seed.max(1);
    let dedup = kind() == "dedup";
    while output.len() < bytes {
        for ch in "café 東京 ".chars() {
            if output.len() + ch.len_utf8() > bytes {
                break;
            }
            output.push(ch);
        }
        for _ in 0..100 {
            if output.len() == bytes {
                break;
            }
            if !dedup {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
            }
            output.push(b"0123456789abcdef"[(state & 15) as usize] as char);
        }
        if output.len() < bytes {
            output.push('\n');
        }
    }
    output
}

pub fn write(path: &Path, bytes: usize) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut offset = 0;
    while offset < bytes {
        let count = (bytes - offset).min(1024 * 1024);
        let seed = if kind() == "dedup" {
            1
        } else {
            0x7192_a335 ^ offset as u64
        };
        file.write_all(text(count, seed).as_bytes())?;
        offset += count;
    }
    file.flush()
}

pub fn contract(backing: &str, production: bool, path: &str) -> serde_json::Value {
    serde_json::json!({"text_backing":backing,"production_path":production,"code_path":path,
        "fixture_kind":kind(),"fixture_revision":REVISION})
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_utf8_is_deterministic_and_distinct_across_store_chunks() {
        let text = super::text(1024 * 1024 + 17, 99);
        assert_eq!(text.len(), 1024 * 1024 + 17);
        assert_eq!(text, super::text(text.len(), 99));
        let chunks: std::collections::HashSet<_> = text.as_bytes().chunks(256 * 1024).collect();
        assert_eq!(chunks.len(), 5);
        assert!(text.contains("東京"));
        for bytes in 0..100 {
            assert_eq!(super::text(bytes, 99).len(), bytes);
        }
    }
}
