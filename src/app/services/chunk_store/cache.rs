//! Byte-bounded LRU indexes: touching a chunk never walks unrelated owners.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

struct CachedBody {
    owner: u64,
    body: Arc<String>,
    access: u64,
}
#[derive(Default)]
struct Owner {
    bytes: usize,
    accesses: BTreeSet<u64>,
}
#[derive(Default)]
pub(super) struct Cache {
    loaded: HashMap<u64, CachedBody>,
    order: BTreeMap<u64, u64>,
    owners: HashMap<u64, Owner>,
    next_access: u64,
    pub(super) bytes: usize,
    pub(super) limit: usize,
    pub(super) per_owner: usize,
}
impl Cache {
    pub(super) fn new(limit: usize, per_owner: usize) -> Self {
        Self {
            limit,
            per_owner,
            ..Self::default()
        }
    }
    fn remove(&mut self, id: u64) {
        if let Some(entry) = self.loaded.remove(&id) {
            self.bytes -= entry.body.len();
            self.order.remove(&entry.access);
            if let Some(owner) = self.owners.get_mut(&entry.owner) {
                owner.bytes -= entry.body.len();
                owner.accesses.remove(&entry.access);
                if owner.accesses.is_empty() {
                    self.owners.remove(&entry.owner);
                }
            }
        }
    }
    pub(super) fn insert(&mut self, id: u64, owner: u64, body: Arc<String>) {
        self.remove(id);
        // Empty text needs no retained body or bookkeeping allocation.
        if body.is_empty() || self.limit == 0 || self.per_owner == 0 {
            return;
        }
        if self.next_access == u64::MAX {
            // Discard cache ownership only; callers' pinned Arcs remain valid.
            self.loaded.clear();
            self.order.clear();
            self.owners.clear();
            self.bytes = 0;
            self.next_access = 0;
        }
        let access = self.next_access;
        self.next_access += 1;
        self.bytes += body.len();
        let state = self.owners.entry(owner).or_default();
        state.bytes += body.len();
        state.accesses.insert(access);
        self.order.insert(access, id);
        self.loaded.insert(
            id,
            CachedBody {
                owner,
                body,
                access,
            },
        );
        self.evict_global();
        self.evict_owner(owner);
    }
    fn evict_global(&mut self) {
        while self.bytes > self.limit {
            let Some((_, &id)) = self.order.first_key_value() else {
                break;
            };
            self.remove(id);
        }
    }
    fn evict_owner(&mut self, id: u64) {
        while let Some(owner) = self.owners.get(&id) {
            if owner.bytes <= self.per_owner {
                break;
            }
            let Some(access) = owner.accesses.first() else {
                break;
            };
            let chunk = self.order[access];
            self.remove(chunk);
        }
    }
    pub(super) fn evict(&mut self) {
        self.evict_global();
        let oversized: Vec<_> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.bytes > self.per_owner)
            .map(|(&id, _)| id)
            .collect();
        for owner in oversized {
            self.evict_owner(owner);
        }
    }
    pub(super) fn owner_loaded(&self, owner: u64) -> usize {
        self.owners
            .get(&owner)
            .map_or(0, |state| state.accesses.len())
    }
    pub(super) fn owner_bytes(&self, owner: u64) -> usize {
        self.owners.get(&owner).map_or(0, |state| state.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, Cache, HashMap};
    #[test]
    fn indexed_cache_matches_reference_lru_under_touches_and_budget_changes() {
        let mut cache = Cache {
            limit: 100,
            per_owner: 30,
            ..Default::default()
        };
        let mut reference: Vec<(u64, u64, usize)> = Vec::new();
        let mut random = 0x0123_4567_89ab_cdef_u64;
        for step in 0..20_000 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            if step % 97 == 0 {
                cache.limit = (random % 150) as usize;
                cache.per_owner = ((random >> 8) % 70) as usize;
                cache.evict();
            } else {
                let id = random % 100;
                let owner = (random >> 8) % 8;
                let len = ((random >> 16) % 20) as usize;
                reference.retain(|&(key, _, _)| key != id);
                if len > 0 {
                    reference.push((id, owner, len));
                }
                cache.insert(id, owner, Arc::new("x".repeat(len)));
            }
            let mut bytes: usize = reference.iter().map(|entry| entry.2).sum();
            let mut owners: HashMap<u64, usize> = HashMap::new();
            for &(_, owner, len) in &reference {
                *owners.entry(owner).or_default() += len;
            }
            reference.retain(|&(_, owner, len)| {
                if bytes > cache.limit || owners[&owner] > cache.per_owner {
                    bytes -= len;
                    *owners.get_mut(&owner).unwrap() -= len;
                    false
                } else {
                    true
                }
            });
            assert_eq!(cache.bytes, bytes);
            assert_eq!(
                cache.order.values().copied().collect::<Vec<_>>(),
                reference.iter().map(|entry| entry.0).collect::<Vec<_>>()
            );
            assert_eq!(cache.loaded.len(), reference.len());
            for owner in 0..8 {
                assert_eq!(
                    cache.owner_bytes(owner),
                    reference
                        .iter()
                        .filter(|entry| entry.1 == owner)
                        .map(|entry| entry.2)
                        .sum::<usize>()
                );
                assert_eq!(
                    cache.owner_loaded(owner),
                    reference.iter().filter(|entry| entry.1 == owner).count()
                );
            }
        }
    }
    #[test]
    fn eviction_and_access_clock_reset_preserve_pinned_text() {
        let mut cache = Cache {
            limit: 10,
            per_owner: 10,
            ..Default::default()
        };
        let text = Arc::new("hello".to_owned());
        cache.insert(1, 1, text.clone());
        cache.next_access = u64::MAX;
        cache.insert(2, 2, Arc::new("world".to_owned()));
        assert_eq!(cache.bytes, 5);
        assert_eq!(&*text, "hello");
        cache.limit = 0;
        cache.evict();
        assert!(cache.loaded.is_empty());
        assert!(cache.order.is_empty());
        assert!(cache.owners.is_empty());
    }
}
