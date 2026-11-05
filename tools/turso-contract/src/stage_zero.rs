//! Remaining standard-WAL experiments. All paths are fresh disposable fixtures.
use super::{
    Result, WalMode,
    live_text::{Reader, Request},
    open,
};
use futures_lite::future::block_on;
use serde_json::json;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

const CHUNK: usize = 256 * 1024;

pub(super) fn run() -> Result<()> {
    let directory = tempfile::tempdir()?;
    chunk_boundaries();
    block_on(history(&directory.path().join("history.db")))?;
    block_on(layouts(&directory.path().join("layouts.db")))?;
    block_on(large_database(&directory.path().join("large.db")))?;
    println!("PASS stage_zero standard WAL; native Windows validation pending separately");
    Ok(())
}

fn sample(name: &str, mut timings: Vec<u128>) -> u128 {
    timings.sort_unstable();
    let p99 = timings[(timings.len() * 99).div_ceil(100) - 1];
    println!(
        "{}",
        json!({"name":name,"samples":timings.len(),"p50_ns":timings[timings.len()/2],"p99_ns":p99,"max_ns":timings.last()})
    );
    p99
}

fn fixture(bytes: usize) -> String {
    let mut state = 918273645_u64;
    let mut text = String::with_capacity(bytes + 4);
    while text.len() < bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        text.push(if state.is_multiple_of(251) {
            'λ'
        } else {
            (32 + state % 95) as u8 as char
        });
    }
    text
}

fn boundaries(text: &str, cdc: bool) -> Vec<(blake3::Hash, usize)> {
    let ends: Vec<_> = if cdc {
        fastcdc::v2020::FastCDC::new(text.as_bytes(), 16 * 1024, 64 * 1024, CHUNK)
            .map(|chunk| chunk.offset + chunk.length)
            .collect()
    } else {
        (CHUNK..text.len())
            .step_by(CHUNK)
            .chain([text.len()])
            .collect()
    };
    let mut start = 0;
    let mut chunks = Vec::new();
    for mut end in ends {
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        if end > start {
            chunks.push((blake3::hash(&text.as_bytes()[start..end]), end - start));
            start = end;
        }
    }
    assert_eq!(start, text.len());
    chunks
}

fn chunk_boundaries() {
    let original = fixture(8 * 1024 * 1024);
    let mut edited = original.clone();
    let mut at = original.len() / 3;
    while !edited.is_char_boundary(at) {
        at += 1;
    }
    edited.insert_str(at, "edited λ\r\n");
    for cdc in [false, true] {
        let timer = Instant::now();
        let old = boundaries(&original, cdc);
        let known: HashSet<_> = old.iter().map(|(hash, _)| *hash).collect();
        let new = boundaries(&edited, cdc);
        let added: usize = new
            .iter()
            .filter(|(hash, _)| !known.contains(hash))
            .map(|(_, len)| len)
            .sum();
        println!(
            "{}",
            json!({"name":"utf8_import_chunking","cdc":cdc,"original_bytes":original.len(),"edited_bytes":edited.len(),"new_unique_bytes":added,"chunks":new.len(),"elapsed_ns":timer.elapsed().as_nanos()})
        );
    }
}

// Both alternatives keep typed entry metadata; only the span encoding differs.
async fn history(path: &Path) -> Result<()> {
    let (_database, connection) = open(path, WalMode::Standard).await?;
    connection.execute_batch("CREATE TABLE entries(id INTEGER PRIMARY KEY,parent_id INTEGER NOT NULL,edited_at INTEGER NOT NULL,global_seq INTEGER NOT NULL,source TEXT NOT NULL,summary TEXT NOT NULL,replayable INTEGER NOT NULL,previous_primary INTEGER NOT NULL,previous_primary_next INTEGER NOT NULL,previous_secondary INTEGER NOT NULL,previous_secondary_next INTEGER NOT NULL,next_primary INTEGER NOT NULL,next_primary_next INTEGER NOT NULL,next_secondary INTEGER NOT NULL,next_secondary_next INTEGER NOT NULL) STRICT;
        CREATE TABLE spans(entry INTEGER NOT NULL REFERENCES entries(id),edit INTEGER NOT NULL,part INTEGER NOT NULL,start_char INTEGER NOT NULL,role INTEGER NOT NULL,buffer INTEGER NOT NULL,start_byte INTEGER NOT NULL,byte_len INTEGER NOT NULL,char_len INTEGER NOT NULL,PRIMARY KEY(entry,edit,part)) STRICT;
        CREATE TABLE packed(entry INTEGER PRIMARY KEY REFERENCES entries(id),body BLOB NOT NULL) STRICT;").await?;
    let mut entry = connection
        .prepare("INSERT INTO entries VALUES(?,?,0,?,'edit','Insert',1,0,0,0,0,0,0,0,0)")
        .await?;
    let mut row = connection
        .prepare("INSERT INTO spans VALUES(?,?,0,?,1,1,?,1,1)")
        .await?;
    let mut packed = connection.prepare("INSERT INTO packed VALUES(?,?)").await?;
    // Seven fixed-width words per span; offsets must not truncate at u32::MAX.
    for many_entries in [true, false] {
        for use_packed in [false, true] {
            let start = Instant::now();
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE; DELETE FROM spans; DELETE FROM packed; DELETE FROM entries;",
                )
                .await?;
            if !many_entries {
                entry.execute((1_i64, 0_i64, 1_i64)).await?;
            }
            let mut all = Vec::new();
            for index in 1..=100_000_i64 {
                let id = if many_entries { index } else { 1 };
                if many_entries {
                    entry.execute((id, id - 1, id)).await?;
                }
                let edit = if many_entries { 0 } else { index - 1 };
                if use_packed {
                    let mut bytes = Vec::with_capacity(56);
                    for value in [edit, index, 1, 1, index, 1, 1] {
                        bytes.extend_from_slice(&value.to_le_bytes());
                    }
                    if many_entries {
                        packed.execute((id, bytes)).await?;
                    } else {
                        all.extend_from_slice(&bytes);
                    }
                } else {
                    row.execute((id, edit, index, index)).await?;
                }
            }
            if use_packed && !many_entries {
                packed.execute((1_i64, all)).await?;
            }
            connection.execute("COMMIT", ()).await?;
            let write_ns = start.elapsed().as_nanos();
            let start = Instant::now();
            let sql = if use_packed {
                "SELECT body FROM packed ORDER BY entry"
            } else {
                "SELECT start_char,role,buffer,start_byte,byte_len,char_len FROM spans ORDER BY entry,edit,part"
            };
            let mut rows = connection.query(sql, ()).await?;
            let mut spans = 0;
            while let Some(row) = rows.next().await? {
                if use_packed {
                    let body = row.get::<Vec<u8>>(0)?;
                    assert_eq!(body.len() % 56, 0);
                    for span in body.chunks_exact(56) {
                        assert_eq!(i64::from_le_bytes(span[40..48].try_into()?), 1);
                        spans += 1;
                    }
                } else {
                    assert!(row.get::<i64>(0)? > 0);
                    assert_eq!(row.get::<i64>(4)?, 1);
                    spans += 1;
                }
            }
            assert_eq!(spans, 100_000);
            println!(
                "{}",
                json!({"name":"history_spans_encoding","many_entries":many_entries,"packed":use_packed,"spans":spans,"write_ns":write_ns,"restore_ns":start.elapsed().as_nanos()})
            );
        }
    }
    Ok(())
}

async fn layouts(path: &Path) -> Result<()> {
    let (_database, c) = open(path, WalMode::Standard).await?;
    c.execute_batch("CREATE TABLE json_tabs(position INTEGER PRIMARY KEY,payload BLOB NOT NULL) STRICT;
    CREATE TABLE tabs(position INTEGER PRIMARY KEY,layout_id INTEGER NOT NULL UNIQUE) STRICT;
    CREATE TABLE layouts(id INTEGER PRIMARY KEY,active_view_id INTEGER NOT NULL,root_pane TEXT NOT NULL) STRICT;
    CREATE TABLE documents(id INTEGER PRIMARY KEY,name TEXT NOT NULL,path TEXT,is_dirty INTEGER NOT NULL,encoding TEXT NOT NULL,line_count INTEGER NOT NULL) STRICT;
    CREATE TABLE views(layout_id INTEGER PRIMARY KEY,id INTEGER NOT NULL,buffer_id INTEGER NOT NULL,zoom REAL NOT NULL,primary_index INTEGER,primary_next INTEGER,secondary_index INTEGER,secondary_next INTEGER) STRICT;").await?;
    let mut json_insert = c.prepare("INSERT INTO json_tabs VALUES(?,?)").await?;
    let mut tab = c.prepare("INSERT INTO tabs VALUES(?,?)").await?;
    let mut layout = c.prepare("INSERT INTO layouts VALUES(?,?,?)").await?;
    let mut document = c
        .prepare("INSERT INTO documents VALUES(?, ?,NULL,0,'UTF-8',1)")
        .await?;
    let mut view = c
        .prepare("INSERT INTO views VALUES(?,?,?,1.0,0,0,0,0)")
        .await?;
    for typed in [false, true] {
        let start = Instant::now();
        c.execute("BEGIN IMMEDIATE", ()).await?;
        for id in 1..=10_000_i64 {
            let root = json!({"Leaf":{"view_id":id}}).to_string();
            let name = format!("document-{id}");
            if typed {
                tab.execute((id - 1, id)).await?;
                layout.execute((id, id, root)).await?;
                document.execute((id, name)).await?;
                view.execute((id, id, id)).await?;
            } else {
                let value = json!({"buffers":[{"id":id,"name":name,"path":null,"is_dirty":false,"encoding":"UTF-8","line_count":1}],"active_view_id":id,"views":[{"id":id,"buffer_id":id,"zoom_factor":1.0,"selection":{"primary_index":0,"primary_next":false,"secondary_index":0,"secondary_next":false}}],"root_pane":serde_json::from_str::<serde_json::Value>(&root)?});
                json_insert
                    .execute((id - 1, serde_json::to_vec(&value)?))
                    .await?;
            }
        }
        c.execute("COMMIT", ()).await?;
        let write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        let mut count = 0;
        if typed {
            for sql in [
                "SELECT * FROM tabs",
                "SELECT * FROM layouts",
                "SELECT * FROM documents",
                "SELECT * FROM views",
            ] {
                let mut rows = c.query(sql, ()).await?;
                while rows.next().await?.is_some() {
                    count += 1;
                }
            }
            assert_eq!(count, 40_000);
        } else {
            let mut rows = c.query("SELECT payload FROM json_tabs", ()).await?;
            while let Some(row) = rows.next().await? {
                std::hint::black_box(serde_json::from_slice::<serde_json::Value>(
                    &row.get::<Vec<u8>>(0)?,
                )?);
                count += 1;
            }
            assert_eq!(count, 10_000);
        }
        let restore_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        let maximum = if typed {
            scalar(&c, "SELECT max(id) FROM views").await?
        } else {
            let mut maximum = 0;
            let mut rows = c.query("SELECT payload FROM json_tabs", ()).await?;
            while let Some(row) = rows.next().await? {
                let value: serde_json::Value = serde_json::from_slice(&row.get::<Vec<u8>>(0)?)?;
                maximum = maximum.max(value["active_view_id"].as_i64().ok_or("missing ID")?);
            }
            maximum
        };
        assert_eq!(maximum, 10_000);
        let reserve_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        c.execute("BEGIN IMMEDIATE", ()).await?;
        if typed {
            c.execute("UPDATE views SET primary_index=1 WHERE layout_id=1", ())
                .await?;
        } else {
            let mut rows = c
                .query("SELECT payload FROM json_tabs WHERE position=0", ())
                .await?;
            let mut value: serde_json::Value = serde_json::from_slice(
                &rows.next().await?.ok_or("missing tab")?.get::<Vec<u8>>(0)?,
            )?;
            drop(rows);
            value["views"][0]["selection"]["primary_index"] = json!(1);
            c.execute(
                "UPDATE json_tabs SET payload=? WHERE position=0",
                [serde_json::to_vec(&value)?],
            )
            .await?;
        }
        c.execute("COMMIT", ()).await?;
        let cursor_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        c.execute("BEGIN IMMEDIATE", ()).await?;
        if typed {
            c.execute("DELETE FROM tabs WHERE position=0", ()).await?;
            // Delete affected positions first, preserving layout content.
            c.execute_batch("CREATE TEMP TABLE reordered AS SELECT position-1 AS position,layout_id FROM tabs; DELETE FROM tabs; INSERT INTO tabs SELECT * FROM reordered; DROP TABLE reordered;").await?;
        } else {
            c.execute_batch("DELETE FROM json_tabs WHERE position=0; CREATE TEMP TABLE reordered AS SELECT position-1 AS position,payload FROM json_tabs; DELETE FROM json_tabs; INSERT INTO json_tabs SELECT * FROM reordered; DROP TABLE reordered;").await?;
        }
        c.execute("COMMIT", ()).await?;
        println!(
            "{}",
            json!({"name":"layout_rows_prototype","typed":typed,"tabs":10_000,"write_ns":write_ns,"restore_ns":restore_ns,"reserve_ids_ns":reserve_ns,"cursor_commit_ns":cursor_ns,"close_first_ns":start.elapsed().as_nanos(),"scope":"structurally equivalent fixture, not complete production SessionBuffer schema"})
        );
    }
    Ok(())
}

async fn scalar(c: &turso::Connection, sql: &str) -> Result<i64> {
    Ok(c.query(sql, ())
        .await?
        .next()
        .await?
        .ok_or("missing scalar")?
        .get(0)?)
}

async fn large_database(path: &Path) -> Result<()> {
    let (_database, writer) = open(path, WalMode::Standard).await?;
    writer
        .execute(
            "CREATE TABLE live_chunks(id INTEGER PRIMARY KEY,body BLOB NOT NULL) STRICT",
            (),
        )
        .await?;
    let body = vec![b'a'; CHUNK];
    let expected = blake3::hash(&body);
    writer.execute("PRAGMA synchronous=NORMAL", ()).await?;
    let mut insert = writer
        .prepare("INSERT INTO live_chunks VALUES(?,?)")
        .await?;
    let timer = Instant::now();
    let mut batches = Vec::new();
    for id in 1..=12_288_i64 {
        let start = Instant::now();
        std::hint::black_box(blake3::hash(&body));
        insert.execute((id, body.as_slice())).await?;
        batches.push(start.elapsed().as_nanos());
    }
    drop(insert);
    let insert_ns = timer.elapsed().as_nanos();
    sample("single_chunk_normal_batch", batches);
    writer.execute("PRAGMA synchronous=FULL", ()).await?;
    writer
        .execute(
            "INSERT INTO live_chunks VALUES(0,?)",
            [b"FULL publication marker".as_slice()],
        )
        .await?;
    println!(
        "{}",
        json!({"name":"large_database_insert","bytes":12_288*CHUNK,"elapsed_ns":insert_ns,"database_bytes":std::fs::metadata(path)?.len(),"wal_bytes":std::fs::metadata(format!("{}-wal",path.display())).map_or(0,|m|m.len())})
    );
    let reader = Reader::new(path, WalMode::Standard)?;
    let mut direct = File::create(path.with_extension("text"))?;
    for _ in 0..512 {
        direct.write_all(&body)?;
    }
    drop(direct);
    let mut direct = File::open(path.with_extension("text"))?;
    let mut file_times = Vec::new();
    let mut database_times = Vec::new();
    writer.execute("PRAGMA synchronous=NORMAL", ()).await?;
    std::thread::scope(|scope| -> Result<()> {
        let commits = scope.spawn(|| {
            block_on(async {
                let mut times = Vec::new();
                for id in 12_289..12_545_i64 {
                    let start = Instant::now();
                    std::hint::black_box(blake3::hash(&body));
                    writer
                        .execute("INSERT INTO live_chunks VALUES(?,?)", (id, body.as_slice()))
                        .await?;
                    times.push(start.elapsed().as_nanos());
                }
                writer.execute("PRAGMA synchronous=FULL", ()).await?;
                writer
                    .execute(
                        "INSERT INTO live_chunks VALUES(-2,?)",
                        [b"FULL publication marker".as_slice()],
                    )
                    .await?;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(times)
            })
        });
        for index in 0..1024_u64 {
            let start = Instant::now();
            let bytes = reader.call(Request::Read((index * 7919 % 12_288 + 1) as i64))?;
            assert_eq!(blake3::hash(&bytes), expected);
            std::str::from_utf8(&bytes)?;
            database_times.push(start.elapsed().as_nanos());
            let start = Instant::now();
            direct.seek(SeekFrom::Start((index * 73 % 512) * CHUNK as u64))?;
            let mut bytes = vec![0; CHUNK];
            direct.read_exact(&mut bytes)?;
            assert_eq!(blake3::hash(&bytes), expected);
            file_times.push(start.elapsed().as_nanos());
        }
        let times = commits.join().map_err(|_| "writer panic")??;
        let maximum = *times.iter().max().ok_or("no chunk batches")?;
        sample("normal_chunk_commit_during_reads", times);
        println!(
            "{}",
            json!({"name":"batch_scheduling_target","target_ns":50_000_000,"observed_max_ns":maximum,"io_overrun":maximum>=50_000_000,"policy":"user-approved scheduling target; bounded batches yield before taking more work; UI responsiveness validated separately"})
        );
        Ok(())
    })?;
    sample("temporary_file_256k_page_cache", file_times);
    let p99 = sample("three_gib_database_reader_verified", database_times);
    assert!(
        p99 < 4_000_000,
        "reader p99 exceeds the unchanged 4 ms gate: {p99} ns"
    );
    drop(reader);
    #[cfg(target_os = "linux")]
    println!(
        "{}",
        json!({"name":"large_database_resources","status":std::fs::read_to_string("/proc/self/status")?.lines().filter(|line|line.starts_with("VmRSS:")||line.starts_with("VmHWM:")).collect::<Vec<_>>(),"io":std::fs::read_to_string("/proc/self/io")?,"page_cache_kib":{"writer":8192,"reader":512},"scope":"process lifetime high water; OS page cache not counted as RSS"})
    );
    Ok(())
}

/// Called only inside a private mount namespace with a size-limited tmpfs.
pub(super) fn enospc(root: &Path) -> Result<()> {
    block_on(async {
        let path = root.join("full.db");
        let (_database, c) = open(&path, WalMode::Standard).await?;
        c.execute(
            "CREATE TABLE bodies(id INTEGER PRIMARY KEY,body BLOB NOT NULL) STRICT",
            (),
        )
        .await?;
        c.execute(
            "INSERT INTO bodies VALUES(0,?)",
            ["retained exact text λ\r\n".as_bytes()],
        )
        .await?;
        let mut filled = false;
        for id in 1..1000_i64 {
            let result = c
                .execute("INSERT INTO bodies VALUES(?,?)", (id, vec![b'x'; CHUNK]))
                .await;
            if let Err(error) = result {
                println!(
                    "ENOSPC observed {error:?}; autocommit={:?}",
                    c.is_autocommit()
                );
                if !c.is_autocommit()? {
                    c.execute("ROLLBACK", ()).await?;
                }
                filled = true;
                break;
            }
        }
        assert!(filled, "test did not exhaust its isolated tmpfs");
        let mut rows = c.query("SELECT body FROM bodies WHERE id=0", ()).await?;
        assert_eq!(
            rows.next()
                .await?
                .ok_or("lost committed body")?
                .get::<Vec<u8>>(0)?,
            "retained exact text λ\r\n".as_bytes()
        );
        println!("PASS actual isolated-filesystem exhaustion retains committed text");
        Ok(())
    })
}
