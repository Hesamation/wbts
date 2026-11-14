//! Compatible with InferPay's inferpay-keyed-merkle-sha256-v1 fact openings.
//! Protocol/algorithm adapted from Taejee Kim's MIT-licensed InferPay.
//! See ../../THIRD_PARTY_NOTICES.md for attribution and the complete license.
use near_sdk::{env, near, serde_json};

pub const DEPTH: usize = 32;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub kind: String,
    pub topic: String,
    pub value: String,
    pub source: String,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub entry: Option<Fact>,
    pub salt: Option<String>,
    pub path: Vec<Option<String>>,
}

pub fn valid_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn valid_key(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 80
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
}

fn node(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend(left);
    bytes.extend(right);
    env::sha256(&bytes)
}

/// A successful result means membership (true) or a proven empty slot (false).
/// It establishes no external truth and does not evaluate arbitrary model text.
pub fn verify(root: &str, key: &str, opening: &Opening) -> Result<bool, &'static str> {
    if !valid_hash(root) || !valid_key(key) || opening.path.len() != DEPTH {
        return Err("invalid root, key or proof depth");
    }
    let present = opening.entry.is_some();
    let mut hash = match (&opening.entry, &opening.salt) {
        (Some(entry), Some(salt)) => {
            if entry.kind != "fact"
                || !entry.topic.is_ascii()
                || entry.topic.len() > 80
                || entry.topic.to_ascii_lowercase() != key
                || entry.value.len() > 512
                || entry.source.len() > 1024
                || !valid_hash(salt)
            {
                return Err("entry does not match requested fact or limits");
            }
            let mut bytes = vec![0];
            bytes.extend(hex::decode(salt).expect("validated salt"));
            // Explicit ordering is essential: SDK serde_json can preserve insertion
            // order. A BTreeMap matches upstream's sorted fixed ASCII field names.
            let fields = std::collections::BTreeMap::from([
                ("kind", &entry.kind),
                ("source", &entry.source),
                ("topic", &entry.topic),
                ("value", &entry.value),
            ]);
            bytes.extend(serde_json::to_vec(&fields).expect("canonical fact"));
            env::sha256(&bytes)
        }
        (None, None) => env::sha256([2]),
        _ => return Err("membership needs a salt; vacancy must not carry one"),
    };
    let mut key_bytes = vec![3];
    key_bytes.extend(serde_json::to_vec(&["fact", key]).expect("canonical key"));
    let digest = env::sha256(&key_bytes);
    let mut slot = u32::from_be_bytes(digest[..4].try_into().expect("hash prefix"));
    let mut empty = env::sha256([2]);
    for sibling in &opening.path {
        let sibling = match sibling {
            Some(text) if valid_hash(text) => hex::decode(text).expect("validated sibling"),
            Some(_) => return Err("invalid sibling hash"),
            None => empty.clone(),
        };
        hash = if slot & 1 == 0 {
            node(&hash, &sibling)
        } else {
            node(&sibling, &hash)
        };
        slot >>= 1;
        empty = node(&empty, &empty);
    }
    if hex::encode(hash) != root {
        return Err("proof does not open requested key at pinned root");
    }
    Ok(present)
}
