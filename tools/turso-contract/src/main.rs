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
    block_on(check_vacuum_into(&path))?;
    for mode in ["commit", "rollback"] {
        let status = Command::new(std::env::current_exe()?)
            .arg(&path)
            .arg(mode)
            .status()?;
        assert!(status.success());
        block_on(check_recovery(&path))?;
    }
    println!(
        "PASS: Turso bindings 0.8.3 commit/rollback, 16 MiB BLOB, abrupt exit, WAL recovery, VACUUM INTO, checkpoint and integrity"
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

async fn check_vacuum_into(path: &Path) -> Result<()> {
    let (_database, connection) = open(path).await?;
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
    let (_backup, copy) = open(&actual).await?;
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
