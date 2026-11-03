//! Append-oriented, copy-on-write metadata. Captures share the root; an edit
//! copies at most 32 elements per tree level, never the entire retained history.
use std::{
    ops::{Index, IndexMut},
    sync::Arc,
};
const WIDTH: usize = 32;

#[derive(Clone)]
enum Node<T> {
    Leaf(Vec<T>),
    Branch(Vec<Arc<Node<T>>>),
}

#[derive(Clone)]
pub struct PersistentVector<T> {
    root: Arc<Node<T>>,
    len: usize,
    level: u32,
}
impl<T> Default for PersistentVector<T> {
    fn default() -> Self {
        Self {
            root: Arc::new(Node::Leaf(Vec::new())),
            len: 0,
            level: 0,
        }
    }
}
impl<T> PersistentVector<T> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    #[must_use]
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }
    #[must_use]
    pub fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|i| self.get(i))
    }
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        (index < self.len).then(|| &self[index])
    }
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator {
        (0..self.len).map(|i| &self[i])
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn partition_point(&self, mut predicate: impl FnMut(&T) -> bool) -> usize {
        let (mut left, mut right) = (0, self.len);
        while left < right {
            let middle = left + (right - left) / 2;
            if predicate(&self[middle]) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }
    pub fn binary_search_by_key<B: Ord>(
        &self,
        key: &B,
        mut map: impl FnMut(&T) -> B,
    ) -> Result<usize, usize> {
        let index = self.partition_point(|value| map(value) < *key);
        if index < self.len && map(&self[index]) == *key {
            Ok(index)
        } else {
            Err(index)
        }
    }
}
impl<T: Clone> PersistentVector<T> {
    pub fn push(&mut self, value: T) {
        if self.len == WIDTH.pow(self.level + 1) {
            self.root = Arc::new(Node::Branch(vec![self.root.clone()]));
            self.level += 1;
        }
        insert(&mut self.root, self.level, self.len, value);
        self.len += 1;
    }
    pub fn pop(&mut self) -> Option<T> {
        let index = self.len.checked_sub(1)?;
        let value = pop(&mut self.root, self.level, index);
        self.len -= 1;
        if self.len == 0 {
            self.clear();
        }
        value
    }
    pub fn last_mut(&mut self) -> Option<&mut T> {
        let index = self.len.checked_sub(1)?;
        Some(&mut self[index])
    }
}
impl<T> Index<usize> for PersistentVector<T> {
    type Output = T;
    fn index(&self, mut index: usize) -> &T {
        assert!(index < self.len, "persistent vector index out of bounds");
        let mut node = self.root.as_ref();
        let mut level = self.level;
        loop {
            match node {
                Node::Leaf(values) => return &values[index],
                Node::Branch(children) => {
                    let capacity = WIDTH.pow(level);
                    node = &children[index / capacity];
                    index %= capacity;
                    level -= 1;
                }
            }
        }
    }
}
impl<T: Clone> IndexMut<usize> for PersistentVector<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        assert!(index < self.len, "persistent vector index out of bounds");
        get_mut(&mut self.root, self.level, index)
    }
}
fn get_mut<T: Clone>(node: &mut Arc<Node<T>>, level: u32, index: usize) -> &mut T {
    match Arc::make_mut(node) {
        Node::Leaf(values) => &mut values[index],
        Node::Branch(children) => {
            let capacity = WIDTH.pow(level);
            get_mut(&mut children[index / capacity], level - 1, index % capacity)
        }
    }
}
fn insert<T: Clone>(node: &mut Arc<Node<T>>, level: u32, index: usize, value: T) {
    match Arc::make_mut(node) {
        Node::Leaf(values) => values.push(value),
        Node::Branch(children) => {
            let capacity = WIDTH.pow(level);
            let child = index / capacity;
            if child == children.len() {
                children.push(Arc::new(if level == 1 {
                    Node::Leaf(Vec::new())
                } else {
                    Node::Branch(Vec::new())
                }));
            }
            insert(&mut children[child], level - 1, index % capacity, value);
        }
    }
}
fn pop<T: Clone>(node: &mut Arc<Node<T>>, level: u32, index: usize) -> Option<T> {
    match Arc::make_mut(node) {
        Node::Leaf(values) => values.pop(),
        Node::Branch(children) => {
            let capacity = WIDTH.pow(level);
            let value = pop(&mut children[index / capacity], level - 1, index % capacity);
            if index.is_multiple_of(capacity) {
                children.pop();
            }
            value
        }
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for PersistentVector<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captures_remain_immutable_across_growth_edits_and_shrinking() {
        let mut values = PersistentVector::default();
        for i in 0..40_000 {
            values.push(i);
        }
        let captured = values.clone();
        for i in (0..40_000).step_by(37) {
            values[i] += 1;
        }
        for i in (0..40_000).rev() {
            assert_eq!(values.pop(), Some(i + usize::from(i % 37 == 0)));
        }
        assert!(values.is_empty());
        values.push(99);
        assert_eq!(values[0], 99);
        assert!(captured.iter().copied().eq(0..40_000));
        assert_eq!(captured.binary_search_by_key(&999, |&i| i), Ok(999));
        assert_eq!(captured.partition_point(|&i| i < 999), 999);
    }
    #[test]
    fn alternating_push_pop_reuses_partial_branch_boundaries() {
        let mut values = PersistentVector::default();
        for boundary in [31, 32, 33, 1023, 1024, 1025] {
            while values.len() < boundary {
                values.push(values.len());
            }
            let captured = values.clone();
            for _ in 0..20 {
                let end = values.len();
                values.push(end);
                assert_eq!(values.pop(), Some(end));
                assert_eq!(values.last(), Some(&(boundary - 1)));
            }
            assert!(values.iter().eq(captured.iter()));
        }
    }
}
