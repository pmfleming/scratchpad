use super::{Arc, BufferState, HashSet, Row, TreeRows, egui, rows};

#[derive(Clone)]
struct VisibleRows {
    structure: (u64, usize, u64),
    collapsed: HashSet<u64>,
    indices: Arc<[usize]>,
}

pub(super) fn load(ctx: &egui::Context, buffer: &BufferState, key: egui::Id) -> (TreeRows, bool) {
    let document = buffer.document();
    let token = document.history_token();
    let previous = ctx
        .data_mut(|d| d.remove_temp::<Option<TreeRows>>(key))
        .flatten();
    let changed = previous.as_ref().is_none_or(|c| c.token != token);
    let structure = document.history_structure_key();
    let topology_changed = previous
        .as_ref()
        .is_none_or(|cache| cache.structure != structure);
    let mut cached = if let Some(mut cached) = previous.filter(|cache| {
        cache.structure == structure
            || (cache.structure.0 == structure.0
                && cache.structure.1 + 1 == structure.1
                && document.history_entries().last().is_some_and(|entry| {
                    cache
                        .rows
                        .last()
                        .is_some_and(|row| row.id == entry.parent_id)
                }))
    }) {
        if topology_changed
            && let Some(entry) = document.history_entries().last()
            && let Some(previous_row) = cached.rows.last()
        {
            let depth = previous_row.depth;
            cached.positions.insert(entry.id, cached.rows.len());
            cached.rows.push(Row {
                id: entry.id,
                depth,
                fork: false,
            });
        }
        cached.structure = structure;
        cached
    } else {
        let rows = rows(buffer);
        let positions = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.id, i))
            .collect();
        TreeRows {
            token,
            rows,
            positions,
            structure,
        }
    };
    cached.token = token;
    (cached, changed)
}

pub(super) fn visible(
    ctx: &egui::Context,
    key: egui::Id,
    cached: &TreeRows,
    collapsed: &HashSet<u64>,
) -> Option<Arc<[usize]>> {
    let structure = cached.structure;
    let visible_key = key.with("visible");
    let previous_visible = ctx.data(|d| d.get_temp::<VisibleRows>(visible_key));
    if collapsed.is_empty() {
        None
    } else if let Some(previous) =
        previous_visible.filter(|v| v.structure == structure && v.collapsed == *collapsed)
    {
        Some(previous.indices)
    } else {
        let mut hidden_depth = None;
        let visible: Vec<_> = cached
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if let Some(depth) = hidden_depth {
                    if row.depth > depth || (!row.fork && row.depth == depth) {
                        return false;
                    }
                    hidden_depth = None;
                }
                if collapsed.contains(&row.id) {
                    hidden_depth = Some(row.depth);
                }
                true
            })
            .map(|(index, _)| index)
            .collect();
        let indices: Arc<[usize]> = visible.into();
        ctx.data_mut(|d| {
            d.insert_temp(
                visible_key,
                VisibleRows {
                    structure,
                    collapsed: collapsed.clone(),
                    indices: indices.clone(),
                },
            )
        });
        Some(indices)
    }
}
