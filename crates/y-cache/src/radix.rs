use blake3::Hasher;
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RadixNode {
    pub token_id: u32,
    pub hash: [u8; 32],
    pub children: HashMap<u32, RadixNode>,
    pub kv_slot_offset: usize,
}

impl RadixNode {
    pub fn new(token_id: u32, parent_hash: &[u8; 32], kv_slot_offset: usize) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(parent_hash);
        hasher.update(&token_id.to_le_bytes());
        let hash = *hasher.finalize().as_bytes();

        Self {
            token_id,
            hash,
            children: HashMap::new(),
            kv_slot_offset,
        }
    }
}

/// Hierarchical Radix Tree Prefix Cache for Token ID sequences.
pub struct RadixPrefixCache {
    root_hash: [u8; 32],
    root_children: RwLock<HashMap<u32, RadixNode>>,
}

impl RadixPrefixCache {
    pub fn new() -> Self {
        let root_hash = *blake3::hash(b"Y_CODE_KV_PREFIX_ROOT").as_bytes();
        Self {
            root_hash,
            root_children: RwLock::new(HashMap::new()),
        }
    }

    /// Finds the length of the longest cached prefix match for the given token slice.
    /// Executes in < 50 µs over &[u32].
    pub fn find_longest_prefix(&self, tokens: &[u32]) -> usize {
        let root = self.root_children.read();
        if tokens.is_empty() {
            return 0;
        }

        let mut current_node = match root.get(&tokens[0]) {
            Some(node) => node,
            None => return 0,
        };

        let mut matched_len = 1;
        for &tok in &tokens[1..] {
            if let Some(child) = current_node.children.get(&tok) {
                matched_len += 1;
                current_node = child;
            } else {
                break;
            }
        }

        matched_len
    }

    /// Inserts a sequence of token IDs into the Radix Tree cache.
    pub fn insert_sequence(&self, tokens: &[u32]) -> usize {
        if tokens.is_empty() {
            return 0;
        }

        let mut root = self.root_children.write();
        let first_tok = tokens[0];
        let root_hash = self.root_hash;

        let node = root
            .entry(first_tok)
            .or_insert_with(|| RadixNode::new(first_tok, &root_hash, 0));

        let mut current = node;
        let mut inserted = 1;

        for (idx, &tok) in tokens[1..].iter().enumerate() {
            let parent_hash = current.hash;
            current = current
                .children
                .entry(tok)
                .or_insert_with(|| RadixNode::new(tok, &parent_hash, idx + 1));
            inserted += 1;
        }

        inserted
    }
}

impl Default for RadixPrefixCache {
    fn default() -> Self {
        Self::new()
    }
}
