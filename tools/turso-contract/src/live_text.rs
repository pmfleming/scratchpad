//! Live-text go/no-go checks. Every database is disposable, never a user session.
use super::{Result, WalMode, open};
use futures_lite::future::block_on;
use std::{
    fs, io,
    path::Path,
    process::Command,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CHUNK_BYTES: usize = 256 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

pub(super) enum Request {
    Read(i64),
    Begin,
    End,
    InjectPanic,
}

/// A bounded hop with its own connection, matching the proposed reader boundary.
/// Dropping the sender joins the reader before any writer/database is closed.
pub(super) struct Reader {
    requests: Option<SyncSender<Request>>,
    replies: Receiver<Result<Vec<u8>>>,
    thread: Option<JoinHandle<()>>,
}

impl Reader {
    pub(super) fn new(path: &Path, mode: WalMode) -> Result<Self> {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let path = path.to_owned();
        let thread = thread::spawn(move || {
            // An open error/panic is returned by the same boundary as read errors.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                block_on(async {
                    let (_database, connection) = open(&path, mode).await?;
                    connection.execute("PRAGMA cache_size=-512", ()).await?;
                    outgoing.send(Ok(Vec::new()))?;
                    while let Ok(request) = incoming.recv() {
                        let result = match request {
                            Request::Read(id) => body(&connection, id).await,
                            Request::Begin => connection
                                .execute("BEGIN", ())
                                .await
                                .map(|_| Vec::new())
                                .map_err(Into::into),
                            Request::End => connection
                                .execute("ROLLBACK", ())
                                .await
                                .map(|_| Vec::new())
                                .map_err(Into::into),
                            Request::InjectPanic => panic!("injected reader panic"),
                        };
                        if outgoing.send(result).is_err() {
                            break;
                        }
                    }
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                })
            }));
            let error = match result {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error,
                Err(_) => io::Error::other("reader thread panicked").into(),
            };
            let _ = outgoing.send(Err(error));
        });
        let reader = Self {
            requests: Some(requests),
            replies,
            thread: Some(thread),
        };
        reader.replies.recv_timeout(DEADLINE)??;
        Ok(reader)
    }

    pub(super) fn call(&self, request: Request) -> Result<Vec<u8>> {
        self.requests
            .as_ref()
            .ok_or("reader stopped")?
            .send(request)?;
        self.replies.recv_timeout(DEADLINE)?
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn body(connection: &turso::Connection, id: i64) -> Result<Vec<u8>> {
    let mut rows = connection
        .query("SELECT body FROM live_chunks WHERE id=?", [id])
        .await?;
    Ok(rows.next().await?.ok_or("missing live chunk")?.get(0)?)
}

async fn scalar(connection: &turso::Connection, sql: &str) -> Result<i64> {
    Ok(connection
        .query(sql, ())
        .await?
        .next()
        .await?
        .ok_or("missing scalar")?
        .get(0)?)
}

async fn checkpoint(connection: &turso::Connection) -> Result<Vec<Option<i64>>> {
    let mut rows = connection
        .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
        .await?;
    let row = rows.next().await?.ok_or("missing checkpoint result")?;
    (0..3)
        .map(|column| match row.get_value(column)? {
            turso::Value::Null => Ok(None),
            turso::Value::Integer(value) => Ok(Some(value)),
            _ => Err("unexpected checkpoint result type".into()),
        })
        .collect()
}

fn wal_bytes(path: &Path) -> u64 {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    fs::metadata(wal).map_or(0, |metadata| metadata.len())
}

fn distribution(name: &str, mode: WalMode, mut samples: Vec<u128>) {
    samples.sort_unstable();
    let p99 = samples[(samples.len() * 99).div_ceil(100).saturating_sub(1)];
    println!(
        "METRIC wal={} name={name} samples={} p50_us={} p99_us={p99} max_us={}",
        mode.name(),
        samples.len(),
        samples[samples.len() / 2],
        samples.last().unwrap()
    );
}

pub(super) fn check(path: &Path, mode: WalMode, overflow: bool) -> Result<()> {
    block_on(async {
        let (_database, writer) = open(path, mode).await?;
        assert_eq!(scalar(&writer, "PRAGMA synchronous").await?, 2);
        assert_eq!(scalar(&writer, "PRAGMA cache_size").await?, -8192);
        assert_eq!(scalar(&writer, "PRAGMA foreign_keys").await?, 1);
        assert_eq!(scalar(&writer, "PRAGMA busy_timeout").await?, 0);
        writer
            .execute(
                "CREATE TABLE live_chunks(id INTEGER PRIMARY KEY, body BLOB NOT NULL) STRICT",
                (),
            )
            .await?;
        let original = vec![b'a'; CHUNK_BYTES];
        writer
            .execute("INSERT INTO live_chunks VALUES(0,?)", [original.as_slice()])
            .await?;
        let reader = Reader::new(path, mode)?;
        assert_eq!(reader.call(Request::Read(0))?, original);
        let changed = vec![b'b'; CHUNK_BYTES];
        writer.execute("BEGIN IMMEDIATE", ()).await?;
        writer
            .execute(
                "UPDATE live_chunks SET body=? WHERE id=0",
                [changed.as_slice()],
            )
            .await?;
        assert_eq!(reader.call(Request::Read(0))?, original, "dirty read");
        let contender = _database.connect()?;
        let start = Instant::now();
        let busy = contender.execute("BEGIN IMMEDIATE", ()).await.unwrap_err();
        assert!(matches!(busy, turso::Error::Busy(_)), "{busy:?}");
        println!(
            "METRIC wal={} name=contending_writer_busy elapsed_us={}",
            mode.name(),
            start.elapsed().as_micros()
        );
        drop(contender);
        writer.execute("COMMIT", ()).await?;
        assert_eq!(reader.call(Request::Read(0))?, changed);

        // Pin a snapshot while commits cross the automatic checkpoint threshold.
        reader.call(Request::Begin)?;
        assert_eq!(reader.call(Request::Read(0))?, changed);
        writer.execute("BEGIN IMMEDIATE", ()).await?;
        writer
            .execute(
                "UPDATE live_chunks SET body=? WHERE id=0",
                [original.as_slice()],
            )
            .await?;
        writer.execute("COMMIT", ()).await?;
        let chunks = if overflow { 4352 } else { 128 };
        let mut commits = Vec::new();
        let mut reads = Vec::new();
        let mut peak_wal = wal_bytes(path);
        for first in (1..=chunks).step_by(4) {
            let start = Instant::now();
            if let Err(error) = writer.execute("BEGIN IMMEDIATE", ()).await {
                let context = format!(
                    "bulk BEGIN at chunk {first}, WAL {} bytes: {error:?}",
                    wal_bytes(path)
                );
                if overflow && matches!(error, turso::Error::Busy(_)) {
                    // Diagnose progress after releasing the old reader, but do
                    // not turn the failed contract into a successful run.
                    let diagnosis: Result<()> = async {
                        reader.call(Request::End)?;
                        let retry = writer.execute("BEGIN IMMEDIATE", ()).await;
                        println!("DIAGNOSTIC wal={} begin_after_snapshot_release={retry:?} autocommit={:?}", mode.name(), writer.is_autocommit());
                        if retry.is_ok() || !writer.is_autocommit()? {
                            writer.execute("ROLLBACK", ()).await?;
                        }
                        drop(reader);
                        drop(writer);
                        drop(_database);
                        let (_reopened, connection) = open(path, mode).await?;
                        connection.execute("BEGIN IMMEDIATE", ()).await?;
                        connection.execute("ROLLBACK", ()).await?;
                        assert_eq!(body(&connection, 0).await?, original);
                        assert_eq!(body(&connection, (first - 1) as i64).await?, original);
                        assert_eq!(checkpoint(&connection).await?[0], Some(0));
                        assert_eq!(wal_bytes(path), 0);
                        Ok(())
                    }.await;
                    println!(
                        "DIAGNOSTIC wal={} reopen_and_verify={diagnosis:?}",
                        mode.name()
                    );
                }
                return Err(context.into());
            }
            for id in first..first + 4 {
                writer
                    .execute(
                        "INSERT INTO live_chunks VALUES(?,?)",
                        (id as i64, original.as_slice()),
                    )
                    .await
                    .map_err(|error| {
                        format!(
                            "bulk INSERT chunk {id}, WAL {} bytes: {error:?}",
                            wal_bytes(path)
                        )
                    })?;
            }
            writer.execute("COMMIT", ()).await.map_err(|error| {
                format!(
                    "bulk COMMIT at chunk {first}, WAL {} bytes: {error:?}",
                    wal_bytes(path)
                )
            })?;
            commits.push(start.elapsed().as_micros());
            peak_wal = peak_wal.max(wal_bytes(path));
            let start = Instant::now();
            assert_eq!(
                reader.call(Request::Read(0)).map_err(|error| format!(
                    "pinned read after chunk {first}, WAL {} bytes: {error}",
                    wal_bytes(path)
                ))?,
                changed,
                "snapshot changed"
            );
            reads.push(start.elapsed().as_micros());
            if overflow && (first - 1) % 256 == 0 {
                println!(
                    "PROGRESS wal={} chunks={} wal_bytes={peak_wal}",
                    mode.name(),
                    first + 3
                );
            }
        }
        distribution("four_chunk_full_commit", mode, commits);
        distribution("pinned_reader_hop", mode, reads);
        println!(
            "METRIC wal={} name=pinned_wal bytes={peak_wal} inserted_bytes={} overflow={overflow}",
            mode.name(),
            chunks * CHUNK_BYTES
        );
        if overflow {
            assert!(
                peak_wal > 262_144 * (4096 + 24),
                "did not exceed the shared frame-index capacity"
            );
        }
        // TRUNCATE can return Busy or a busy result while the snapshot is pinned.
        let pinned = checkpoint(&writer).await?;
        assert_eq!(pinned[0], Some(1), "pinned TRUNCATE must report Busy");
        println!(
            "CHECK wal={} name=pinned_truncate result={pinned:?}",
            mode.name()
        );
        assert_eq!(reader.call(Request::Read(0))?, changed);
        reader.call(Request::End)?;
        assert_eq!(reader.call(Request::Read(0))?, original);
        let result = checkpoint(&writer).await?;
        assert_eq!(
            result[0],
            Some(0),
            "TRUNCATE still busy after snapshot release"
        );
        assert_eq!(wal_bytes(path), 0, "WAL not truncated");

        // Small per-reader page cache + shuffled unique IDs avoids its warm pages.
        // The OS page cache is intentionally not dropped: these are page-cache
        // reads, not claims about cold physical storage.
        let mut hop = Vec::new();
        let mut direct = Vec::new();
        for n in 0..512 {
            let id = ((n * 73) % chunks + 1) as i64;
            let start = Instant::now();
            assert_eq!(reader.call(Request::Read(id))?, original);
            hop.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert_eq!(body(&writer, id).await?, original);
            direct.push(start.elapsed().as_micros());
        }
        distribution("chunk_read_hop_page_cache", mode, hop);
        distribution("chunk_read_direct_page_cache", mode, direct);

        // Run the writer on a separate thread while foreground requests keep
        // crossing the reader hop. These really overlap commits/checkpoints;
        // the pinned-snapshot checks above measure isolation between commits.
        thread::scope(|scope| -> Result<()> {
            let worker = scope.spawn(|| -> Result<Vec<u128>> {
                block_on(async {
                    let mut samples = Vec::new();
                    for id in 1..=128 {
                        let start = Instant::now();
                        writer.execute("BEGIN IMMEDIATE", ()).await?;
                        writer
                            .execute(
                                "UPDATE live_chunks SET body=? WHERE id=?",
                                (original.as_slice(), id as i64),
                            )
                            .await?;
                        writer.execute("COMMIT", ()).await?;
                        samples.push(start.elapsed().as_micros());
                    }
                    Ok(samples)
                })
            });
            let mut samples = Vec::new();
            let mut count = 0;
            while !worker.is_finished() {
                let start = Instant::now();
                assert_eq!(
                    reader.call(Request::Read((count * 73 % chunks + 1) as i64))?,
                    original
                );
                samples.push(start.elapsed().as_micros());
                count += 1;
            }
            let commits = worker.join().map_err(|_| "concurrent writer panicked")??;
            assert!(!samples.is_empty(), "no reads overlapped writes");
            distribution("reader_hop_during_commits", mode, samples);
            distribution("single_chunk_full_commit_with_reader", mode, commits);
            Ok(())
        })?;
        // Test the containment boundary, rather than claiming to provoke a
        // particular engine bug. A replacement connection must still read text.
        let failure = reader.call(Request::InjectPanic).unwrap_err();
        assert!(failure.to_string().contains("reader thread panicked"));
        drop(reader);
        let replacement = Reader::new(path, mode)?;
        assert_eq!(replacement.call(Request::Read(0))?, original);
        drop(replacement);
        println!(
            "CHECK wal={} name=injected_reader_panic_contained PASS",
            mode.name()
        );

        // A page ceiling exercises DatabaseFull without exhausting the host disk.
        // Physical ENOSPC/fsync failures still require separate fault injection.
        let page_count = scalar(&writer, "PRAGMA page_count").await?;
        let ceiling = scalar(
            &writer,
            &format!("PRAGMA max_page_count={}", page_count + 1),
        )
        .await?;
        assert_eq!(ceiling, page_count + 1);
        writer.execute("BEGIN IMMEDIATE", ()).await?;
        let full = writer
            .execute(
                "INSERT INTO live_chunks VALUES(-4,?)",
                [vec![b'f'; 4 * CHUNK_BYTES]],
            )
            .await
            .unwrap_err();
        assert!(matches!(full, turso::Error::DatabaseFull(_)), "{full:?}");
        if !writer.is_autocommit()? {
            writer.execute("ROLLBACK", ()).await?;
        }
        assert_eq!(body(&writer, 0).await?, original);
        scalar(&writer, "PRAGMA max_page_count=4294967294").await?;
        println!(
            "CHECK wal={} name=page_limit_database_full_preserves_text PASS",
            mode.name()
        );
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })?;
    let status = Command::new(std::env::current_exe()?)
        .arg("--abrupt")
        .arg(path)
        .arg("normal-full")
        .arg(mode.name())
        .status()?;
    assert!(status.success(), "NORMAL/FULL crash child failed");
    block_on(async {
        let (_database, connection) = open(path, mode).await?;
        assert_eq!(body(&connection, -1).await?, b"normal chunk");
        assert_eq!(body(&connection, -2).await?, b"full manifest");
        for id in 1_000_000..1_000_128_i64 {
            assert_eq!(body(&connection, id).await?, vec![b'n'; CHUNK_BYTES]);
        }
        assert_eq!(
            scalar(&connection, "SELECT count(*) FROM live_chunks WHERE id=-3").await?,
            0,
            "uncommitted row survived"
        );
        assert_eq!(checkpoint(&connection).await?[0], Some(0));
        assert_eq!(wal_bytes(path), 0, "crashed reader slots blocked truncate");
        let mut rows = connection.query("PRAGMA integrity_check", ()).await?;
        assert_eq!(
            rows.next()
                .await?
                .ok_or("missing integrity result")?
                .get::<String>(0)?,
            "ok"
        );
        assert!(rows.next().await?.is_none());
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })?;
    println!(
        "CHECK wal={} name=normal_full_process_exit_and_active_slots PASS (not a power-loss test)",
        mode.name()
    );
    Ok(())
}

pub(super) fn abrupt_normal_full(path: &Path, mode: WalMode) -> Result<()> {
    block_on(async {
        let (_database, connection) = open(path, mode).await?;
        let reader = Reader::new(path, mode)?;
        connection.execute("PRAGMA synchronous=NORMAL", ()).await?;
        for id in 1_000_000..1_000_128_i64 {
            if id == 1_000_064 {
                reader.call(Request::Begin)?;
                reader.call(Request::Read(0))?;
            }
            connection
                .execute(
                    "INSERT INTO live_chunks VALUES(?,?)",
                    (id, vec![b'n'; CHUNK_BYTES]),
                )
                .await?;
        }
        connection
            .execute(
                "INSERT INTO live_chunks VALUES(-1,?)",
                [b"normal chunk".as_slice()],
            )
            .await?;
        connection.execute("PRAGMA synchronous=FULL", ()).await?;
        connection
            .execute(
                "INSERT INTO live_chunks VALUES(-2,?)",
                [b"full manifest".as_slice()],
            )
            .await?;
        connection.execute("BEGIN IMMEDIATE", ()).await?;
        connection
            .execute(
                "INSERT INTO live_chunks VALUES(-3,?)",
                [b"uncommitted".as_slice()],
            )
            .await?;
        // Both reader and writer slots remain active; bypass every destructor.
        std::process::exit(0);
        #[allow(unreachable_code)]
        Ok(())
    })
}
