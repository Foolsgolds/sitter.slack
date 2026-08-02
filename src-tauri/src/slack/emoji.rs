//! Port of `MainWindow.ResolveEmojiUrl` / `LoadCustomEmojiAsync`: Slack's
//! `emoji.list` returns a flat `name -> value` map where `value` may be
//! `alias:<other-name>`. We resolve every alias chain up front (with cycle
//! detection) into a flat `name -> URL` map, rather than resolving lazily
//! per-render.

use std::collections::{HashMap, HashSet};

const ALIAS_PREFIX: &str = "alias:";

/// Flattens a raw Slack emoji map (name -> value-or-alias) into name -> URL.
/// Entries whose alias chain is empty, missing, or cyclic are dropped.
pub fn flatten_emoji_map(raw: &HashMap<String, String>) -> HashMap<String, String> {
    let mut resolved = HashMap::with_capacity(raw.len());
    for name in raw.keys() {
        if let Some(url) = resolve_emoji_url(raw, name) {
            resolved.insert(name.clone(), url);
        }
    }
    resolved
}

fn resolve_emoji_url(raw: &HashMap<String, String>, name: &str) -> Option<String> {
    let mut visited = HashSet::new();
    let mut current = name.to_string();
    loop {
        if !visited.insert(current.clone()) {
            return None; // cycle detected
        }
        let value = raw.get(&current)?;
        if value.is_empty() {
            return None;
        }
        match value.strip_prefix(ALIAS_PREFIX) {
            Some(alias_target) => current = alias_target.to_string(),
            None => return Some(value.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_value_resolves_as_is() {
        let mut raw = HashMap::new();
        raw.insert("smile".to_string(), "https://x/smile.png".to_string());
        let resolved = flatten_emoji_map(&raw);
        assert_eq!(resolved.get("smile").unwrap(), "https://x/smile.png");
    }

    #[test]
    fn alias_chain_resolves_to_final_url() {
        let mut raw = HashMap::new();
        raw.insert("a".to_string(), "alias:b".to_string());
        raw.insert("b".to_string(), "alias:c".to_string());
        raw.insert("c".to_string(), "https://x/c.png".to_string());
        let resolved = flatten_emoji_map(&raw);
        assert_eq!(resolved.get("a").unwrap(), "https://x/c.png");
    }

    #[test]
    fn alias_cycle_is_dropped_not_infinite_loop() {
        let mut raw = HashMap::new();
        raw.insert("a".to_string(), "alias:b".to_string());
        raw.insert("b".to_string(), "alias:a".to_string());
        let resolved = flatten_emoji_map(&raw);
        assert!(resolved.get("a").is_none());
        assert!(resolved.get("b").is_none());
    }

    #[test]
    fn missing_alias_target_is_dropped() {
        let mut raw = HashMap::new();
        raw.insert("a".to_string(), "alias:missing".to_string());
        let resolved = flatten_emoji_map(&raw);
        assert!(resolved.get("a").is_none());
    }
}
