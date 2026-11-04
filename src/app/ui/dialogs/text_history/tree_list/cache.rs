use super::{
    Arc, BufferState, Edge, HashMap, HashSet, HistoryToken, Prepared, Row, TreeRows, ViewRows, egui,
};
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

pub(super) fn key(buffer: &BufferState) -> egui::Id {
    egui::Id::unique(("history.tree.rows", buffer.id))
}

pub(super) fn tree(ctx: &egui::Context, buffer: &BufferState) -> Arc<TreeRows> {
    let structure = buffer.document().history_structure_key();
    let key = key(buffer);
    if let Some(tree) = ctx.data(|d| d.get_temp::<Arc<TreeRows>>(key))
        && tree.structure == structure
    {
        return tree;
    }
    let tree = Arc::new(build(buffer));
    ctx.data_mut(|d| d.insert_temp(key, tree.clone()));
    tree
}

fn build(buffer: &BufferState) -> TreeRows {
    let entries = buffer.document().history_entries();
    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    for entry in entries.iter() {
        children.entry(entry.parent_id).or_default().push(entry.id);
    }
    let mut rows: Vec<Row> = Vec::with_capacity(entries.len() + 1);
    let mut positions: HashMap<u64, usize> = HashMap::new();
    let mut pending = vec![(0, 0, false)];
    // Iterative enter/leave traversal also handles very deep linear histories.
    while let Some((id, parent, leaving)) = pending.pop() {
        if leaving {
            let index = positions[&id];
            rows[index].end = rows.len() - 1;
            continue;
        }
        positions.insert(id, rows.len());
        rows.push(Row {
            id,
            parent,
            lane: 0,
            fork: id != 0 && children.get(&parent).is_some_and(|c| c.len() > 1),
            end: 0,
        });
        pending.push((id, parent, true));
        if let Some(next) = children.get(&id) {
            pending.extend(next.iter().rev().map(|&child| (child, id, false)));
        }
    }
    let mut occupied = BinaryHeap::new();
    let mut free = BTreeSet::new();
    let mut lanes = 1;
    for index in 0..rows.len() {
        while occupied
            .peek()
            .is_some_and(|Reverse((end, _))| *end < index)
        {
            if let Some(Reverse((_, lane))) = occupied.pop() {
                free.insert(lane);
            }
        }
        if let Some(next) = children.get(&rows[index].id) {
            for (n, child) in next.iter().enumerate() {
                let child_index = positions[child];
                let lane = if n == 0 {
                    rows[index].lane
                } else {
                    let lane = free.pop_first().unwrap_or_else(|| {
                        let lane = lanes;
                        lanes += 1;
                        lane
                    });
                    occupied.push(Reverse((rows[child_index].end, lane)));
                    lane
                };
                rows[child_index].lane = lane;
            }
        }
    }
    TreeRows {
        structure: buffer.document().history_structure_key(),
        rows,
        positions,
        max_lane: lanes - 1,
        forks: children
            .into_iter()
            .filter_map(|(id, children)| (children.len() > 1).then_some(id))
            .collect(),
    }
}

pub(super) fn prepare(ctx: &egui::Context, buffer: &BufferState) -> Prepared {
    let key = key(buffer);
    let token = buffer.document().history_token();
    let changed = ctx.data(|d| d.get_temp::<HistoryToken>(key.with("token"))) != Some(token);
    let tree = tree(ctx, buffer);
    let mut collapsed = ctx
        .data(|d| d.get_temp::<HashSet<u64>>(key.with("collapsed")))
        .unwrap_or_default();
    collapsed.retain(|id| {
        tree.positions.contains_key(id)
            && (!changed || !buffer.document().is_history_node_applied(*id))
    });
    ctx.data_mut(|d| {
        d.insert_temp(key.with("collapsed"), collapsed.clone());
        d.insert_temp(key.with("token"), token);
    });
    let previous = ctx.data(|d| d.get_temp::<Arc<ViewRows>>(key.with("view")));
    let view = if let Some(view) =
        previous.filter(|view| Arc::ptr_eq(&view.tree, &tree) && view.collapsed == collapsed)
    {
        view
    } else {
        let mut indices = Vec::new();
        let mut i = 0;
        while i < tree.rows.len() {
            indices.push(i);
            i = if collapsed.contains(&tree.rows[i].id) {
                tree.rows[i].end + 1
            } else {
                i + 1
            };
        }
        indices.reverse();
        let positions: HashMap<_, _> = indices
            .iter()
            .enumerate()
            .map(|(i, &row)| (tree.rows[row].id, i))
            .collect();
        let edges = indices
            .iter()
            .enumerate()
            .filter_map(|(a, &i)| {
                let row = &tree.rows[i];
                if row.id == 0 {
                    return None;
                }
                let b = *positions.get(&row.parent)?;
                Some(Edge {
                    id: row.id,
                    a,
                    b,
                    lane: row.lane,
                    parent_lane: tree.rows[tree.positions[&row.parent]].lane,
                })
            })
            .collect::<Vec<_>>()
            .into();
        let view = Arc::new(ViewRows {
            collapsed,
            tree,
            indices,
            positions,
            edges,
        });
        ctx.data_mut(|d| d.insert_temp(key.with("view"), view.clone()));
        view
    };
    Prepared { view, changed }
}
