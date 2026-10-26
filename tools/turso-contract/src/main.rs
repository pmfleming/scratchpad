//! Isolated release-engine contract. Does not open a user's session database.
use futures_lite::future::block_on;
use std::{error::Error, path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 3 {
        block_on(abrupt_writer(Path::new(&args[1]), args[2] == "commit"))?;
        // Deliberately bypass destructors and connection checkpoint-on-close.
        std::process::exit(0);
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("session.db");
    block_on(check_transactions(&path))?;
    for mode in ["commit", "rollback"] {
        let status = Command::new(std::env::current_exe()?)
            .arg(&path)
            .arg(mode)
            .status()?;
        assert!(status.success());
        block_on(check_recovery(&path))?;
    }
    println!(
        "PASS: Turso bindings 0.8.0 commit/rollback, 16 MiB BLOB, abrupt exit, WAL recovery, checkpoint and integrity"
    );
    Ok(())
}

async fn open(path: &Path) -> Result<(turso::Database, turso::Connection)> {
    let database = turso::Builder::new_local(path.to_str().ok_or("non-UTF8 path")?)
        .build()
        .await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA synchronous=FULL", ()).await?;
    Ok((database, connection))
}

async fn check_transactions(path: &Path) -> Result<()> {
    let (_database, connection) = open(path).await?;
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

async fn abrupt_writer(path: &Path, commit: bool) -> Result<()> {
    let (_database, connection) = open(path).await?;
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

async fn check_recovery(path: &Path) -> Result<()> {
    let (_database, connection) = open(path).await?;
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
