//! B-tree operations: insert, lookup, delete, range scan.
//!
//! This module implements a B-tree that operates on nodes. For now it uses
//! an in-memory store; later it will be integrated with the buffer manager.
//!
//! # Design
//!
//! - **Insert**: Find leaf, insert key-value, split if full, propagate splits up
//! - **Lookup**: Traverse from root to leaf, binary search at each level
//! - **Delete**: Find leaf, mark tombstone, merge if underfull
//! - **Range scan**: Cursor-based forward/reverse iteration
//!
//! # Out-of-Place Writes
//!
//! In the full implementation, "modify" operations create new page versions.
//! For now, we mutate nodes directly. The PMT integration comes later.

use crate::btree::node::{InsertError, Node, SplitError, ValueRef};

/// Page ID type (index into the page store).
pub type PageId = u32;

/// Result of a lookup operation.
#[derive(Debug)]
pub enum LookupResult<'a> {
    /// Key found with inline value.
    Found(&'a [u8]),
    /// Key found with blob pointer.
    Blob(crate::btree::node::BlobPointer),
    /// Key is deleted (tombstone).
    Deleted,
    /// Key not found.
    NotFound,
}

/// A simple in-memory B-tree.
///
/// This is the core B-tree logic. In the full implementation, this will be
/// backed by the buffer manager and PMT. For now, it stores nodes in a Vec.
pub struct BTree {
    /// All nodes stored in memory. Index 0 is always the root.
    nodes: Vec<Node>,
    /// Page ID of the root node.
    root: PageId,
}

impl Default for BTree {
    fn default() -> Self {
        Self::new()
    }
}

impl BTree {
    /// Create a new empty B-tree with a single leaf root.
    pub fn new() -> Self {
        let root = Node::new_leaf();
        Self {
            nodes: vec![root],
            root: 0,
        }
    }

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get a reference to a node by page ID.
    pub fn node(&self, id: PageId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    /// Get a mutable reference to a node by page ID.
    fn node_mut(&mut self, id: PageId) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    /// Allocate a new node and return its page ID.
    fn alloc_node(&mut self, node: Node) -> PageId {
        let id = self.nodes.len() as PageId;
        self.nodes.push(node);
        id
    }

    /// Insert a key-value pair into the B-tree.
    ///
    /// If the key already exists, returns an error.
    /// May cause node splits if the leaf is full.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<(), BTreeError> {
        let leaf_id = self.find_leaf(key);
        let result = self.node_mut(leaf_id)
            .expect("leaf_id should be valid")
            .insert(key, value);

        match result {
            Ok(()) => Ok(()),
            Err(InsertError::PageFull) => {
                self.split_and_insert_leaf(leaf_id, key, value)?;
                Ok(())
            }
            Err(InsertError::DuplicateKey(_)) => Err(BTreeError::DuplicateKey),
            Err(e) => Err(BTreeError::InsertFailed(e)),
        }
    }

    /// Lookup a key in the B-tree.
    pub fn lookup(&self, key: &[u8]) -> LookupResult<'_> {
        let leaf_id = self.find_leaf(key);
        let node = self.node(leaf_id).expect("leaf_id should be valid");

        match node.search(key) {
            Ok(idx) => match node.value(idx) {
                Some(ValueRef::Inline(data)) => LookupResult::Found(data),
                Some(ValueRef::Blob(ptr)) => LookupResult::Blob(ptr),
                Some(ValueRef::Tombstone) => LookupResult::Deleted,
                None => LookupResult::NotFound,
            },
            Err(_) => LookupResult::NotFound,
        }
    }

    /// Delete a key by inserting a tombstone.
    ///
    /// Returns true if the key was found (even if already deleted).
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, BTreeError> {
        let leaf_id = self.find_leaf(key);
        let node = self.node_mut(leaf_id).expect("leaf_id should be valid");

        if node.search(key).is_err() {
            return Ok(false);
        }

        node.insert_tombstone(key)
            .map_err(BTreeError::InsertFailed)?;
        Ok(true)
    }

    /// Create a forward range scan over [start, end).
    pub fn range_scan(&self, start: &[u8], end: &[u8]) -> RangeScan<'_> {
        RangeScan::new(self, start.to_vec(), end.to_vec())
    }

    // -- Internal helpers --

    /// Find the leaf node where `key` should reside.
    fn find_leaf(&self, key: &[u8]) -> PageId {
        let mut current = self.root;

        loop {
            let node = self.node(current).expect("current should be valid");
            if node.is_leaf() {
                return current;
            }

            // Internal node: find the child to descend into.
            //
            // Layout: leftmost_child key_0 child_1 key_1 child_2 ...
            //
            // For key < key_0: go to leftmost_child
            // For key_0 <= key < key_1: go to child_1
            // etc.
            let count = node.count();
            let mut child_id = node.leftmost_child();

            for i in 0..count {
                if let Some(sep_key) = node.key(i)
                    && key < sep_key.as_slice()
                {
                    break;
                }
                child_id = node.child_id(i).unwrap_or(0);
            }

            current = child_id as u32;
        }
    }

    /// Split a leaf node that's full and insert the key-value.
    fn split_and_insert_leaf(
        &mut self,
        leaf_id: PageId,
        key: &[u8],
        value: &[u8],
    ) -> Result<(), BTreeError> {
        let (median_key, right_node) = {
            let leaf = self.node_mut(leaf_id).expect("leaf_id should be valid");
            leaf.split().map_err(BTreeError::SplitFailed)?
        };

        let right_id = self.alloc_node(right_node);

        let target_id = if key >= median_key.as_slice() {
            right_id
        } else {
            leaf_id
        };

        self.node_mut(target_id)
            .expect("target_id should be valid")
            .insert(key, value)
            .map_err(BTreeError::InsertFailed)?;

        if leaf_id == self.root {
            self.create_new_root(leaf_id, &median_key, right_id);
        } else {
            let parent_id = self.find_parent(self.root, leaf_id)
                .expect("parent should exist for non-root node");
            self.insert_into_internal(parent_id, &median_key, right_id)?;
        }

        Ok(())
    }

    /// Create a new root with two children.
    fn create_new_root(&mut self, left_id: PageId, key: &[u8], right_id: PageId) {
        let mut new_root = Node::new_internal();

        // For internal nodes, child_id(i) is the child AFTER key_i.
        // The leftmost child (before key 0) is stored in leftmost_child.
        new_root.set_leftmost_child(left_id as u64);
        new_root.insert_child(key, right_id as u64)
            .expect("new root should have space");

        let new_root_id = self.alloc_node(new_root);

        self.node_mut(left_id).expect("left_id should be valid").set_parent_id(new_root_id);
        self.node_mut(right_id).expect("right_id should be valid").set_parent_id(new_root_id);

        self.root = new_root_id;
    }

    /// Find the parent of a given node (by DFS).
    fn find_parent(&self, current: PageId, target: PageId) -> Option<PageId> {
        if current == target {
            return None;
        }

        let node = self.node(current)?;
        if node.is_leaf() {
            return None;
        }

        for i in 0..node.count() {
            if let Some(child_id) = node.child_id(i) {
                if child_id as u32 == target {
                    return Some(current);
                }
                if let Some(parent) = self.find_parent(child_id as u32, target) {
                    return Some(parent);
                }
            }
        }

        None
    }

    /// Insert a key and right child into an internal node.
    fn insert_into_internal(
        &mut self,
        parent_id: PageId,
        key: &[u8],
        right_child_id: PageId,
    ) -> Result<(), BTreeError> {
        let result = self.node_mut(parent_id)
            .expect("parent_id should be valid")
            .insert_child(key, right_child_id as u64);

        match result {
            Ok(()) => Ok(()),
            Err(InsertError::PageFull) => {
                self.split_internal(parent_id, key, right_child_id)
            }
            Err(e) => Err(BTreeError::InsertFailed(e)),
        }
    }

    /// Split an internal node and insert the key.
    fn split_internal(
        &mut self,
        node_id: PageId,
        key: &[u8],
        right_child_id: PageId,
    ) -> Result<(), BTreeError> {
        let (median_key, right_node) = {
            let node = self.node_mut(node_id).expect("node_id should be valid");
            node.split().map_err(BTreeError::SplitFailed)?
        };

        let right_id = self.alloc_node(right_node);

        let target_id = if key >= median_key.as_slice() {
            right_id
        } else {
            node_id
        };

        self.node_mut(target_id)
            .expect("target_id should be valid")
            .insert_child(key, right_child_id as u64)
            .map_err(BTreeError::InsertFailed)?;

        if node_id == self.root {
            self.create_new_root(node_id, &median_key, right_id);
        } else {
            let parent_id = self.find_parent(self.root, node_id)
                .expect("parent should exist for non-root node");
            self.insert_into_internal(parent_id, &median_key, right_id)?;
        }

        Ok(())
    }
}

/// Error from B-tree operations.
#[derive(Debug, thiserror::Error)]
pub enum BTreeError {
    #[error("duplicate key")]
    DuplicateKey,
    #[error("insert failed: {0}")]
    InsertFailed(InsertError),
    #[error("split failed: {0}")]
    SplitFailed(SplitError),
}

/// Cursor for range scanning the B-tree.
pub struct RangeScan<'a> {
    tree: &'a BTree,
    start: Vec<u8>,
    end: Vec<u8>,
    current_node: PageId,
    current_index: usize,
    done: bool,
}

impl<'a> RangeScan<'a> {
    fn new(tree: &'a BTree, start: Vec<u8>, end: Vec<u8>) -> Self {
        let leaf_id = tree.find_leaf(&start);
        let node = tree.node(leaf_id).expect("leaf_id should be valid");

        let start_index = match node.search(&start) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        Self {
            tree,
            start,
            end,
            current_node: leaf_id,
            current_index: start_index,
            done: false,
        }
    }
}

impl<'a> Iterator for RangeScan<'a> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            let node = self.tree.node(self.current_node)?;

            if self.current_index < node.count() {
                let key = node.key(self.current_index)?;

                if key >= self.end {
                    self.done = true;
                    return None;
                }

                self.current_index += 1;

                if let Some(ValueRef::Inline(value)) = node.value(self.current_index - 1)
                    && key >= self.start
                {
                    return Some((key, value.to_vec()));
                }
                continue;
            }

            // No sibling pointers yet, so we can't traverse to the next leaf.
            self.done = true;
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_insert_and_lookup() {
        let mut tree = BTree::new();

        tree.insert(b"hello", b"world").unwrap();
        tree.insert(b"foo", b"bar").unwrap();
        tree.insert(b"aaa", b"bbb").unwrap();

        assert!(matches!(tree.lookup(b"hello"), LookupResult::Found(b"world")));
        assert!(matches!(tree.lookup(b"foo"), LookupResult::Found(b"bar")));
        assert!(matches!(tree.lookup(b"aaa"), LookupResult::Found(b"bbb")));
        assert!(matches!(tree.lookup(b"missing"), LookupResult::NotFound));
    }

    #[test]
    fn test_btree_duplicate_key() {
        let mut tree = BTree::new();

        tree.insert(b"key", b"val1").unwrap();
        assert!(matches!(tree.insert(b"key", b"val2"), Err(BTreeError::DuplicateKey)));
    }

    #[test]
    fn test_btree_delete() {
        let mut tree = BTree::new();

        tree.insert(b"key", b"value").unwrap();
        assert!(matches!(tree.lookup(b"key"), LookupResult::Found(_)));

        tree.delete(b"key").unwrap();
        assert!(matches!(tree.lookup(b"key"), LookupResult::Deleted));

        assert_eq!(tree.delete(b"missing").unwrap(), false);
    }

    #[test]
    fn test_btree_split() {
        let mut tree = BTree::new();

        for i in 0..500 {
            let key = format!("key_{:06}", i);
            let val = format!("val_{:06}", i);
            tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }

        for i in 0..500 {
            let key = format!("key_{:06}", i);
            let val = format!("val_{:06}", i);
            assert!(matches!(
                tree.lookup(key.as_bytes()),
                LookupResult::Found(v) if v == val.as_bytes()
            ));
        }

        assert!(tree.node_count() > 1);
    }

    #[test]
    fn test_btree_range_scan() {
        let mut tree = BTree::new();

        tree.insert(b"a", b"1").unwrap();
        tree.insert(b"b", b"2").unwrap();
        tree.insert(b"c", b"3").unwrap();
        tree.insert(b"d", b"4").unwrap();
        tree.insert(b"e", b"5").unwrap();

        let results: Vec<_> = tree.range_scan(b"b", b"e").collect();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, b"b");
        assert_eq!(results[1].0, b"c");
        assert_eq!(results[2].0, b"d");
    }

    #[test]
    fn test_btree_many_inserts() {
        let mut tree = BTree::new();

        for i in 0..500 {
            let key = format!("key_{:06}", i);
            let val = format!("val_{:06}", i);
            tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }

        for i in 0..500 {
            let key = format!("key_{:06}", i);
            assert!(matches!(tree.lookup(key.as_bytes()), LookupResult::Found(_)));
        }
    }

    #[test]
    fn test_btree_sorted_order() {
        let mut tree = BTree::new();

        for i in (0..50).rev() {
            let key = format!("key_{:04}", i);
            let val = format!("val_{:04}", i);
            tree.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }

        for i in 0..50 {
            let key = format!("key_{:04}", i);
            assert!(matches!(tree.lookup(key.as_bytes()), LookupResult::Found(_)));
        }
    }
}
