//! Indexed, bounded manifest reads. Immutable text bodies remain lazy.
use super::content::{
    decode_piece_parts, decode_storage_block, decode_tail, restore_tree, restore_tree_structure,
};
use super::{Database, error, invalid, quarantine::damaged};
use crate::app::domain::buffer::PieceTreeLite;
use std::{collections::HashMap, io};

const PIECES: &str = "SELECT p.id,p.part,p.bytes,p.digest,p.body FROM json_each(?) requested CROSS JOIN document_pieces p ON p.id=CAST(requested.value AS TEXT) ORDER BY p.id,p.part";
const BLOCKS: &str = "SELECT t.temp_id,t.buffer,t.position,t.start_byte,c.id,c.digest,c.byte_len,c.char_len,c.newline_count,c.is_ascii,c.ends_with_lf FROM json_each(?) requested CROSS JOIN document_text t ON t.temp_id=CAST(requested.value AS TEXT) LEFT JOIN chunks c ON c.id=t.chunk_id ORDER BY t.temp_id,t.buffer,t.position";
const TAILS: &str = "SELECT t.temp_id,t.start_byte,t.offset,t.body,t.digest FROM json_each(?) requested CROSS JOIN document_tail t ON t.temp_id=CAST(requested.value AS TEXT) ORDER BY t.temp_id,t.offset";

impl Database {
    pub(super) async fn read_reference_trees(
        &self,
        ids: &[String],
    ) -> io::Result<HashMap<String, PieceTreeLite>> {
        self.read_reference_trees_inner(ids, true).await
    }
    pub(super) async fn read_recovery_tree(&self, id: &str) -> io::Result<PieceTreeLite> {
        self.read_reference_trees_inner(&[id.to_owned()], false)
            .await?
            .remove(id)
            .ok_or_else(|| damaged(id, "missing piece metadata"))
    }
    async fn read_reference_trees_inner(
        &self,
        ids: &[String],
        check_length: bool,
    ) -> io::Result<HashMap<String, PieceTreeLite>> {
        let encoded = serde_json::to_string(ids).map_err(invalid)?;
        let store = self.chunk_store()?;
        let owners: HashMap<_, _> = ids
            .iter()
            .map(|id| {
                (
                    id.as_str(),
                    crate::app::domain::buffer::text_cache::new_owner(),
                )
            })
            .collect();
        let pieces = self
            .query_collect(PIECES, [encoded.as_str()], |r| {
                let id = r.get::<String>(0).map_err(error)?;
                let row = (|| -> io::Result<_> {
                    Ok((
                        r.get::<i64>(1).map_err(error)?,
                        r.get::<i64>(2).map_err(error)?,
                        r.get::<Vec<u8>>(3).map_err(error)?,
                        r.get::<Vec<u8>>(4).map_err(error)?,
                    ))
                })()
                .map_err(|e| damaged(&id, e))?;
                Ok((id, row))
            })
            .await?;
        let blocks = self
            .query_collect(BLOCKS, [encoded.as_str()], |r| {
                let id = r.get::<String>(0).map_err(error)?;
                let owner = *owners
                    .get(id.as_str())
                    .ok_or_else(|| invalid("unrequested content owner"))?;
                let block =
                    decode_storage_block(r, 1, &store, owner).map_err(|e| damaged(&id, e))?;
                Ok((id, block))
            })
            .await?;
        let tails = self
            .query_collect(TAILS, [encoded.as_str()], |r| {
                let id = r.get::<String>(0).map_err(error)?;
                let row = (|| -> io::Result<_> {
                    Ok((
                        usize::try_from(r.get::<i64>(1).map_err(error)?).map_err(invalid)?,
                        usize::try_from(r.get::<i64>(2).map_err(error)?).map_err(invalid)?,
                        r.get::<Vec<u8>>(3).map_err(error)?,
                        r.get::<Vec<u8>>(4).map_err(error)?,
                    ))
                })()
                .map_err(|e| damaged(&id, e))?;
                Ok((id, row))
            })
            .await?;
        let mut pieces = group(pieces);
        let mut blocks = group(blocks);
        let mut tails = group(tails);
        let mut trees = HashMap::new();
        for id in ids {
            let Some((bytes, encoded_pieces, _)) =
                decode_piece_parts(id, pieces.remove(id).unwrap_or_default())?
            else {
                continue;
            };
            let tail = decode_tail(id, tails.remove(id).unwrap_or_default())?;
            let blocks = blocks.remove(id).unwrap_or_default();
            let tree = if check_length {
                restore_tree(id, store.clone(), bytes, &encoded_pieces, blocks, tail)?
            } else {
                restore_tree_structure(id, store.clone(), &encoded_pieces, blocks, tail)?
            };
            trees.insert(id.clone(), tree);
        }
        Ok(trees)
    }
}
fn group<T>(rows: Vec<(String, T)>) -> HashMap<String, Vec<T>> {
    let mut grouped = HashMap::new();
    for (id, row) in rows {
        grouped.entry(id).or_insert_with(Vec::new).push(row);
    }
    grouped
}

#[cfg(test)]
mod tests {
    #[test]
    fn batch_content_queries_seek_document_indexes() {
        futures_lite::future::block_on(async {
            let (_root, db) = super::super::tests::database().await;
            for (alias, query) in [
                ("p", super::PIECES),
                ("t", super::BLOCKS),
                ("t", super::TAILS),
            ] {
                let mut rows = db
                    .conn
                    .query(&format!("EXPLAIN QUERY PLAN {query}"), ())
                    .await
                    .unwrap();
                let mut details = Vec::new();
                while let Some(row) = rows.next().await.unwrap() {
                    details.push(row.get::<String>(3).unwrap());
                }
                assert!(
                    details
                        .iter()
                        .any(|line| line.starts_with(&format!("SEARCH {alias} USING INDEX"))),
                    "{details:?}"
                );
                assert!(
                    !details
                        .iter()
                        .any(|line| line.starts_with(&format!("SCAN {alias}"))),
                    "{details:?}"
                );
            }
        });
    }
}
