//! Bounded multi-row inserts for new records inside the existing transaction.
use super::Database;
use std::io;

impl Database {
    pub(super) async fn insert_rows<const N: usize>(
        &self,
        prefix: &str,
        rows: impl IntoIterator<Item = io::Result<[Result<turso::Value, turso::Error>; N]>>,
    ) -> io::Result<()> {
        // At most 32 rows are copied into parameters at once. Power-of-two tails
        // bound the number of cached VM shapes independently of workload size.
        let mut pending = Vec::with_capacity(32 * N);
        for row in rows {
            for value in row? {
                pending.push(value.map_err(super::error)?);
            }
            if pending.len() == 32 * N {
                self.insert_values(prefix, N, std::mem::take(&mut pending))
                    .await?;
                pending = Vec::with_capacity(32 * N);
            }
        }
        while !pending.is_empty() {
            let count = 1usize << (pending.len() / N).ilog2();
            let tail = pending.split_off(count * N);
            self.insert_values(prefix, N, pending).await?;
            pending = tail;
        }
        Ok(())
    }

    async fn insert_values(
        &self,
        prefix: &str,
        width: usize,
        values: Vec<turso::Value>,
    ) -> io::Result<()> {
        let row = format!("({})", vec!["?"; width].join(","));
        let sql = format!(
            "{prefix} VALUES{}",
            vec![row; values.len() / width].join(",")
        );
        self.execute_cached(&sql, turso::params_from_iter(values))
            .await
    }
}
