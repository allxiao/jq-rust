//! Object type for JV
//!
//! JSON objects with ordered keys and copy-on-write semantics.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::Jv;

/// JSON object value
///
/// Uses BTreeMap for ordered keys (jq preserves insertion order in output,
/// but sorts keys for equality comparison).
/// Reference counting with copy-on-write for efficient cloning.
#[derive(Debug, Clone)]
pub struct JvObject {
    // Using BTreeMap for consistent ordering
    inner: Rc<RefCell<BTreeMap<String, Jv>>>,
}

impl JvObject {
    /// Create a new empty object
    pub fn new() -> Self {
        JvObject {
            inner: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    /// Create an object from key-value pairs
    pub fn from_pairs<I, K, V>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Jv>,
    {
        let map: BTreeMap<String, Jv> = iter
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        JvObject {
            inner: Rc::new(RefCell::new(map)),
        }
    }

    /// Create an object from pre-collected entries (avoids intermediate cloning)
    #[inline]
    pub fn from_entries_vec(entries: Vec<(String, Jv)>) -> Self {
        let map: BTreeMap<String, Jv> = entries.into_iter().collect();
        JvObject {
            inner: Rc::new(RefCell::new(map)),
        }
    }

    /// Get the number of keys
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Get value by key
    pub fn get(&self, key: &str) -> Option<Jv> {
        self.inner.borrow().get(key).cloned()
    }

    /// Check if key exists
    pub fn has(&self, key: &str) -> bool {
        self.inner.borrow().contains_key(key)
    }

    /// Set a key-value pair
    pub fn set(&mut self, key: &str, value: Jv) {
        self.make_unique();
        self.inner.borrow_mut().insert(key.to_string(), value);
    }

    /// Remove a key
    pub fn delete(&mut self, key: &str) -> Option<Jv> {
        self.make_unique();
        self.inner.borrow_mut().remove(key)
    }

    /// Get all keys as an array of strings
    pub fn keys(&self) -> Vec<String> {
        self.inner.borrow().keys().cloned().collect()
    }

    /// Get all keys as Jv strings
    pub fn keys_jv(&self) -> Vec<Jv> {
        self.inner.borrow().keys().map(Jv::string).collect()
    }

    /// Get all values
    pub fn values(&self) -> impl Iterator<Item = Jv> + '_ {
        let keys: Vec<String> = self.inner.borrow().keys().cloned().collect();
        keys.into_iter()
            .map(move |k| self.inner.borrow().get(&k).cloned().unwrap_or(Jv::Null))
    }

    /// Iterate over key-value pairs
    pub fn iter(&self) -> impl Iterator<Item = (String, Jv)> + '_ {
        let pairs: Vec<(String, Jv)> = self
            .inner
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        pairs.into_iter()
    }

    /// Merge with another object (other's values take precedence)
    pub fn merge(&self, other: &JvObject) -> JvObject {
        let mut result = self.inner.borrow().clone();
        for (k, v) in other.inner.borrow().iter() {
            result.insert(k.clone(), v.clone());
        }
        JvObject {
            inner: Rc::new(RefCell::new(result)),
        }
    }

    /// Add a key-value pair (returns new object, jq style)
    pub fn add(&self, key: &str, value: Jv) -> JvObject {
        let mut result = self.inner.borrow().clone();
        result.insert(key.to_string(), value);
        JvObject {
            inner: Rc::new(RefCell::new(result)),
        }
    }

    /// Remove a key (returns new object, jq style)
    pub fn del(&self, key: &str) -> JvObject {
        let mut result = self.inner.borrow().clone();
        result.remove(key);
        JvObject {
            inner: Rc::new(RefCell::new(result)),
        }
    }

    /// Convert to entries array: [{key, value}, ...]
    pub fn to_entries(&self) -> Vec<Jv> {
        self.inner
            .borrow()
            .iter()
            .map(|(k, v)| {
                let mut entry = JvObject::new();
                entry.set("key", Jv::string(k));
                entry.set("name", Jv::string(k)); // jq also includes "name"
                entry.set("value", v.clone());
                Jv::Object(entry)
            })
            .collect()
    }

    /// Create from entries array
    pub fn from_entries(entries: &[Jv]) -> Option<JvObject> {
        let mut obj = JvObject::new();
        for entry in entries {
            if let Jv::Object(e) = entry {
                // jq accepts "key", "name", or "k" for the key
                let key = e
                    .get("key")
                    .or_else(|| e.get("name"))
                    .or_else(|| e.get("k"))?;

                // jq accepts "value" or "v" for the value
                let value = e.get("value").or_else(|| e.get("v")).unwrap_or(Jv::Null);

                if let Jv::String(k) = key {
                    obj.set(k.as_str(), value);
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(obj)
    }

    /// Ensure unique ownership for mutation
    fn make_unique(&mut self) {
        if Rc::strong_count(&self.inner) > 1 {
            let cloned = self.inner.borrow().clone();
            self.inner = Rc::new(RefCell::new(cloned));
        }
    }
}

impl Default for JvObject {
    fn default() -> Self {
        JvObject::new()
    }
}

impl PartialEq for JvObject {
    fn eq(&self, other: &Self) -> bool {
        let a = self.inner.borrow();
        let b = other.inner.borrow();
        a.len() == b.len() && a.iter().all(|(k, v)| b.get(k) == Some(v))
    }
}

impl Eq for JvObject {}

impl PartialOrd for JvObject {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JvObject {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare by keys first, then values
        let a = self.inner.borrow();
        let b = other.inner.borrow();

        let keys_cmp = a.keys().cmp(b.keys());
        if keys_cmp != Ordering::Equal {
            return keys_cmp;
        }

        for key in a.keys() {
            let av = a.get(key).unwrap();
            let bv = b.get(key).unwrap();
            let cmp = av.cmp(bv);
            if cmp != Ordering::Equal {
                return cmp;
            }
        }

        Ordering::Equal
    }
}

impl Hash for JvObject {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash in sorted key order for consistency
        for (k, v) in self.inner.borrow().iter() {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl fmt::Display for JvObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        let map = self.inner.borrow();
        for (i, (k, v)) in map.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "\"{}\":{}", k.escape_default(), v)?;
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let obj = JvObject::new();
        assert!(obj.is_empty());
        assert_eq!(obj.len(), 0);
    }

    #[test]
    fn test_set_get() {
        let mut obj = JvObject::new();
        obj.set("name", Jv::string("test"));
        obj.set("value", Jv::from_i64(42));

        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("name"), Some(Jv::string("test")));
        assert_eq!(obj.get("value"), Some(Jv::from_i64(42)));
        assert_eq!(obj.get("missing"), None);
    }

    #[test]
    fn test_has() {
        let mut obj = JvObject::new();
        obj.set("key", Jv::Null);

        assert!(obj.has("key"));
        assert!(!obj.has("missing"));
    }

    #[test]
    fn test_delete() {
        let mut obj = JvObject::new();
        obj.set("a", Jv::from_i64(1));
        obj.set("b", Jv::from_i64(2));

        obj.delete("a");
        assert!(!obj.has("a"));
        assert!(obj.has("b"));
    }

    #[test]
    fn test_merge() {
        let mut obj1 = JvObject::new();
        obj1.set("a", Jv::from_i64(1));
        obj1.set("b", Jv::from_i64(2));

        let mut obj2 = JvObject::new();
        obj2.set("b", Jv::from_i64(20));
        obj2.set("c", Jv::from_i64(3));

        let merged = obj1.merge(&obj2);
        assert_eq!(merged.get("a"), Some(Jv::from_i64(1)));
        assert_eq!(merged.get("b"), Some(Jv::from_i64(20))); // obj2's value
        assert_eq!(merged.get("c"), Some(Jv::from_i64(3)));
    }

    #[test]
    fn test_keys() {
        let mut obj = JvObject::new();
        obj.set("b", Jv::Null);
        obj.set("a", Jv::Null);
        obj.set("c", Jv::Null);

        let keys = obj.keys();
        // BTreeMap keeps keys sorted
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_copy_on_write() {
        let mut obj1 = JvObject::new();
        obj1.set("a", Jv::from_i64(1));

        let mut obj2 = obj1.clone();
        obj2.set("b", Jv::from_i64(2));

        assert!(!obj1.has("b")); // Original unchanged
        assert!(obj2.has("b"));
    }
}
