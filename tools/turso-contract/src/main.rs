//! Isolated release-engine contract. Does not open a user's session database.
mod live_text;

use futures_lite::future::block_on;
use std::{error::Error, path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy, Debug)]
enum WalMode {
    Standard,
    Multiprocess,
}

impl WalMode {
    fn parse(value: &std::ffi::OsStr) -> Result<Self> {
        match value.to_str() {
            Some("standard") => Ok(Self::Standard),
            Some("multiprocess") if cfg!(target_os = "linux") => Ok(Self::Multiprocess),
            _ => Err("WAL mode must be standard, or multiprocess on Linux".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Multiprocess => "multiprocess",
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--abrupt") {
        if args.len() != 5 {
            return Err(
                "internal usage: --abrupt PATH commit|rollback|normal-full WAL-MODE".into(),
            );
        }
        let mode = WalMode::parse(&args[4])?;
        match args[3].to_str() {
            Some("commit") => block_on(abrupt_writer(Path::new(&args[2]), true, mode)),
            Some("rollback") => block_on(abrupt_writer(Path::new(&args[2]), false, mode)),
            Some("normal-full") => live_text::abrupt_normal_full(Path::new(&args[2]), mode),
            _ => Err("unknown abrupt-exit scenario".into()),
        }?;
        return Err("abrupt-exit scenario returned without exiting".into());
    }
    if args.len() != 1 && (args.len() < 3 || args[1] != "--wal-mode" || args.len() > 4) {
        return Err(
            "usage: scratchpad-turso-spike --wal-mode standard|multiprocess [--wal-overflow]"
                .into(),
        );
    }
    let wal_mode = args
        .get(2)
        .map_or(Ok(WalMode::Standard), |arg| WalMode::parse(arg))?;
    let overflow = match args.get(3).map(|arg| arg.to_str()) {
        None => false,
        Some(Some("--wal-overflow")) => true,
        _ => return Err("unknown option (expected --wal-overflow)".into()),
    };
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("session.db");
    println!(
        "CONTRACT wal={} io=syscall synchronous=FULL cache_kib=8192 foreign_keys=ON",
        wal_mode.name()
    );
    block_on(check_transactions(&path, wal_mode))?;
    block_on(check_vacuum_into(&path, wal_mode))?;
    for action in ["commit", "rollback"] {
        let status = Command::new(std::env::current_exe()?)
            .arg("--abrupt")
            .arg(&path)
            .arg(action)
            .arg(wal_mode.name())
            .status()?;
        assert!(status.success());
        block_on(check_recovery(&path, wal_mode))?;
    }
    live_text::check(&directory.path().join("live.db"), wal_mode, overflow)?;
    println!(
        "PASS wal={}: Turso bindings 0.8.3 transactions, BLOBs, abrupt exit, recovery, VACUUM INTO, concurrent readers, Busy, checkpoints and integrity",
        wal_mode.name()
    );
    Ok(())
}

async fn open(path: &Path, mode: WalMode) -> Result<(turso::Database, turso::Connection)> {
    let database = turso::Builder::new_local(path.to_str().ok_or("non-UTF8 path")?)
        .with_io(turso::IoBackend::Syscall)
        .experimental_multiprocess_wal(matches!(mode, WalMode::Multiprocess))
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA cache_size=-8192; PRAGMA foreign_keys=ON;")
        .await?;
    Ok((database, connection))
}

async fn check_transactions(path: &Path, mode: WalMode) -> Result<()> {
    let (_database, connection) = open(path, mode).await?;
    connection
        .execute_batch("CREATE TABLE content(id INTEGER PRIMARY KEY, body BLOB); BEGIN IMMEDIATE;")
        .await?;
    let body = vec![0x42; 16 * 1024 * 1024];
    connection
        .execute("INSERT INTO content VALUES(1, ?)", [body.as_slice()])
        .await?;
    connection.execute("COMMIT", ()).await?;
    connection
        .execute_batch("BEGIN IMMEDIATE; DELETE FROM content; ROLLBACK;")
        .await?;
    let mut rows = connection.query("SELECT body FROM content", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("missing content")?
            .get::<Vec<u8>>(0)?,
        body
    );
    Ok(())
}

async fn check_vacuum_into(path: &Path, mode: WalMode) -> Result<()> {
    let (_database, connection) = open(path, mode).await?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA application_id=1397769040; PRAGMA user_version=9;
         CREATE TABLE parents(id INTEGER PRIMARY KEY) STRICT;
         CREATE TABLE children(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES parents(id)) STRICT;
         INSERT INTO parents VALUES(1); INSERT INTO children VALUES(1,1);"
    ).await?;
    for (sql, expected) in [
        ("PRAGMA application_id", 1397769040),
        ("PRAGMA user_version", 9),
    ] {
        assert_eq!(
            connection
                .query(sql, ())
                .await?
                .next()
                .await?
                .ok_or("missing source header")?
                .get::<i64>(0)?,
            expected,
            "source header before VACUUM INTO"
        );
    }
    // Keep the source open, including committed WAL content. Paths are SQL
    // literals (bind parameters are not supported by VACUUM INTO on 0.8.3).
    let destination = path.with_file_name("backup'quoted.db");
    let quoted = destination
        .to_str()
        .ok_or("non-UTF8 backup path")?
        .replace('\'', "''");
    connection
        .execute(&format!("VACUUM INTO '{quoted}'"), ())
        .await?;
    // 0.8.3 trims literal delimiters but does not unescape doubled apostrophes.
    // Characterize that limitation, rather than opening the intended path and
    // accidentally creating an empty database that looks like a broken backup.
    let actual = path.with_file_name("backup''quoted.db");
    assert!(
        !destination.exists(),
        "review quote handling when upgrading Turso"
    );
    assert!(actual.is_file(), "VACUUM output is missing");
    eprintln!(
        "VACUUM INTO caveat: 0.8.3 retains doubled apostrophes in output filenames; application adoption requires a path workaround"
    );
    let (_backup, copy) = open(&actual, mode).await?;
    for (sql, expected) in [
        ("PRAGMA application_id", 1397769040),
        ("PRAGMA user_version", 9),
        ("SELECT count(*) FROM children WHERE parent=1", 1),
        (
            "SELECT length(body) FROM content WHERE id=1",
            16 * 1024 * 1024,
        ),
    ] {
        assert_eq!(
            copy.query(sql, ())
                .await?
                .next()
                .await?
                .ok_or("missing backup result")?
                .get::<i64>(0)?,
            expected
        );
    }
    assert_eq!(
        copy.query("SELECT body FROM content WHERE id=1", ())
            .await?
            .next()
            .await?
            .ok_or("missing copied body")?
            .get::<Vec<u8>>(0)?,
        vec![0x42; 16 * 1024 * 1024]
    );
    let mut rows = copy.query("PRAGMA integrity_check", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("missing integrity result")?
            .get::<String>(0)?,
        "ok"
    );
    assert!(rows.next().await?.is_none());
    drop(rows);
    assert!(
        copy.query("PRAGMA foreign_key_check", ())
            .await?
            .next()
            .await?
            .is_none()
    );
    copy.execute("PRAGMA foreign_keys=ON", ()).await?;
    assert!(
        copy.execute("INSERT INTO children VALUES(2,999)", ())
            .await
            .is_err()
    );
    assert!(
        copy.execute("INSERT INTO parents VALUES('not an integer')", ())
            .await
            .is_err()
    );
    let schema = copy
        .query("SELECT sql FROM sqlite_master WHERE name='children'", ())
        .await?
        .next()
        .await?
        .ok_or("missing STRICT schema")?
        .get::<String>(0)?;
    assert!(schema.contains("STRICT"));
    // A destination with data must never be silently replaced.
    assert!(
        connection
            .execute(&format!("VACUUM INTO '{quoted}'"), ())
            .await
            .is_err()
    );
    Ok(())
}

async fn abrupt_writer(path: &Path, commit: bool, mode: WalMode) -> Result<()> {
    let (_database, connection) = open(path, mode).await?;
    connection.execute("BEGIN IMMEDIATE", ()).await?;
    connection
        .execute(
            "UPDATE content SET body = ? WHERE id = 1",
            [b"committed\0\r\n".as_slice()],
        )
        .await?;
    if commit {
        connection.execute("COMMIT", ()).await?;
    } else {
        connection.execute("DELETE FROM content", ()).await?;
    }
    // Exit here too: returning would run destructors on these local objects.
    std::process::exit(0);
}

async fn check_recovery(path: &Path, mode: WalMode) -> Result<()> {
    let (_database, connection) = open(path, mode).await?;
    let mut rows = connection.query("SELECT body FROM content", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("lost committed content")?
            .get::<Vec<u8>>(0)?,
        b"committed\0\r\n"
    );
    drop(rows);
    let mut checkpoint = connection
        .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
        .await?;
    while checkpoint.next().await?.is_some() {}
    drop(checkpoint);
    let mut rows = connection.query("PRAGMA integrity_check", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("missing integrity result")?
            .get::<String>(0)?,
        "ok"
    );
    Ok(())
}
