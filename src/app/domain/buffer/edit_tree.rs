//! File history topology. Root is zero; text payloads belong to the document.
//! Cached routes make adjacent undo/redo constant-time without scanning siblings.
use std::collections::BTreeMap;

pub type NodeId = u64;
pub const ROOT: NodeId = 0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditTree {
    parents: BTreeMap<NodeId, (NodeId, usize)>,
    route: Vec<NodeId>,
    position: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navigation {
    pub undo: Vec<NodeId>,
    pub redo: Vec<NodeId>,
    route: Option<Vec<NodeId>>,
    position: usize,
}

impl EditTree {
    #[must_use]
    pub fn now(&self) -> NodeId {
        self.position.checked_sub(1).map_or(ROOT, |i| self.route[i])
    }

    #[must_use]
    pub fn tip(&self) -> NodeId {
        self.route.last().copied().unwrap_or(ROOT)
    }

    #[must_use]
    pub fn route(&self) -> &[NodeId] {
        &self.route
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(&id).map(|&(parent, _)| parent)
    }

    #[must_use]
    pub fn is_on_route(&self, id: NodeId) -> bool {
        id == ROOT
            || self
                .parents
                .get(&id)
                .is_some_and(|&(_, depth)| self.route.get(depth - 1) == Some(&id))
    }

    #[must_use]
    pub fn is_applied(&self, id: NodeId) -> bool {
        id == ROOT
            || self.parents.get(&id).is_some_and(|&(_, depth)| {
                depth <= self.position && self.route.get(depth - 1) == Some(&id)
            })
    }

    #[must_use]
    pub fn redo_target(&self) -> Option<NodeId> {
        self.route.get(self.position).copied()
    }

    pub fn links(&self) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        self.parents.iter().map(|(&id, &(parent, _))| (id, parent))
    }

    /// IDs increase globally within this tree, including after branching.
    pub fn append(&mut self, id: NodeId) -> bool {
        if id == ROOT
            || self
                .parents
                .last_key_value()
                .is_some_and(|(&last, _)| id <= last)
        {
            return false;
        }
        self.parents.insert(id, (self.now(), self.position + 1));
        self.route.truncate(self.position);
        self.route.push(id);
        self.position += 1;
        true
    }

    /// Remove only the most recently allocated leaf when an open typing group
    /// cancels itself. Navigation seals groups, so shared ancestors never enter here.
    pub fn cancel_open_leaf(&mut self, id: NodeId) -> bool {
        if self.now() != id
            || self.tip() != id
            || self.parents.last_key_value().map(|(&last, _)| last) != Some(id)
        {
            return false;
        }
        self.parents.remove(&id);
        self.route.pop();
        self.position -= 1;
        true
    }

    fn path(&self, mut id: NodeId) -> Option<Vec<NodeId>> {
        let mut route = Vec::new();
        while id != ROOT {
            route.push(id);
            id = self.parent(id)?;
        }
        route.reverse();
        Some(route)
    }

    /// Compute a transition before changing text or the current cursor.
    #[must_use]
    pub fn navigation(&self, target: NodeId) -> Option<Navigation> {
        let depth = if target == ROOT {
            0
        } else {
            self.parents.get(&target)?.1
        };
        if target == ROOT || self.route.get(depth - 1) == Some(&target) {
            return Some(Navigation {
                undo: if depth < self.position {
                    self.route[depth..self.position]
                        .iter()
                        .rev()
                        .copied()
                        .collect()
                } else {
                    Vec::new()
                },
                redo: if depth > self.position {
                    self.route[self.position..depth].to_vec()
                } else {
                    Vec::new()
                },
                route: None,
                position: depth,
            });
        }
        let target_path = self.path(target)?;
        let route = if self.route.starts_with(&target_path) {
            self.route.clone()
        } else {
            // Stable IDs reflect creation order. A descendant must have a larger
            // ID than its parent; the newest descendant is necessarily a leaf.
            let mut descendants = std::collections::BTreeSet::from([target]);
            for (&id, (parent, _)) in self.parents.range(target..) {
                if descendants.contains(parent) {
                    descendants.insert(id);
                }
            }
            self.path(*descendants.last()?)?
        };
        let common = self.route[..self.position]
            .iter()
            .zip(&target_path)
            .take_while(|(a, b)| a == b)
            .count();
        Some(Navigation {
            undo: self.route[common..self.position]
                .iter()
                .rev()
                .copied()
                .collect(),
            redo: target_path[common..].to_vec(),
            route: Some(route),
            position: target_path.len(),
        })
    }

    pub fn commit(&mut self, navigation: Navigation) {
        if let Some(route) = navigation.route {
            self.route = route;
        }
        self.position = navigation.position;
    }

    /// Restore only validated, parent-before-child links and a consistent cursor.
    pub fn restore(
        links: impl IntoIterator<Item = (NodeId, NodeId)>,
        now: NodeId,
        tip: NodeId,
    ) -> Option<Self> {
        let mut tree = Self::default();
        for (id, parent) in links {
            if id == ROOT
                || parent >= id
                || (parent != ROOT && !tree.parents.contains_key(&parent))
                || tree.parents.contains_key(&id)
            {
                return None;
            }
            let depth = if parent == ROOT {
                1
            } else {
                tree.parents[&parent].1 + 1
            };
            tree.parents.insert(id, (parent, depth));
        }
        tree.route = tree.path(tip)?;
        tree.position = if now == ROOT {
            0
        } else {
            tree.route.iter().position(|&id| id == now)? + 1
        };
        Some(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn go(tree: &mut EditTree, target: NodeId) -> Navigation {
        let navigation = tree.navigation(target).unwrap();
        tree.commit(navigation.clone());
        navigation
    }

    #[test]
    fn branches_share_ancestors_and_remember_the_selected_continuation() {
        let mut tree = EditTree::default();
        for id in 1..=3 {
            assert!(tree.append(id));
        }
        assert_eq!(go(&mut tree, 1).undo, [3, 2]);
        assert!(tree.append(4));
        assert_eq!(tree.parent(4), Some(1));
        go(&mut tree, 1);
        assert_eq!(tree.redo_target(), Some(4));
        let change = go(&mut tree, 3);
        assert_eq!(change.redo, [2, 3]);
        let change = go(&mut tree, 4);
        assert_eq!(change.undo, [3, 2]);
        assert_eq!(change.redo, [4]);
        assert_eq!(tree.links().count(), 4);
        go(&mut tree, ROOT);
        let restored = EditTree::restore(tree.links(), tree.now(), tree.tip()).unwrap();
        assert_eq!(restored, tree);
        assert_eq!(restored.route(), [1, 4]);
    }

    #[test]
    fn invalid_links_and_positions_do_not_form_a_tree() {
        assert!(EditTree::restore([(1, 0), (2, 3)], 1, 2).is_none());
        assert!(EditTree::restore([(1, 0), (1, 0)], 1, 1).is_none());
        assert!(EditTree::restore([(1, 0), (2, 0)], 1, 2).is_none());
        let mut tree = EditTree::default();
        tree.append(1);
        tree.append(2);
        assert!(tree.navigation(99).is_none());
        assert!(!tree.cancel_open_leaf(1));
        assert_eq!(tree.now(), 2);
        assert!(tree.cancel_open_leaf(2));
        assert_eq!(tree.now(), 1);
    }

    #[test]
    fn seeded_routes_match_an_independent_parent_model() {
        use rand::{RngExt, SeedableRng, rngs::StdRng};
        for seed in 0..16 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut tree = EditTree::default();
            let mut parents = vec![0];
            let mut current = 0;
            for _ in 0..300 {
                if rng.random_bool(0.6) {
                    let id = parents.len() as u64;
                    parents.push(current);
                    assert!(tree.append(id));
                    current = id;
                } else {
                    current = rng.random_range(0..parents.len()) as u64;
                    go(&mut tree, current);
                }
                let mut path = Vec::new();
                let mut node = current;
                while node != 0 {
                    path.push(node);
                    node = parents[node as usize];
                }
                path.reverse();
                assert_eq!(&tree.route()[..tree.position()], path);
                assert_eq!(tree.now(), current);
                assert_eq!(tree.links().count(), parents.len() - 1);
            }
        }
    }
}
