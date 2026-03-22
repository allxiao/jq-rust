//! String type for JV
//!
//! JSON strings with UTF-8 encoding.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

/// JSON string value
///
/// Uses reference counting for efficient cloning.
#[derive(Debug, Clone)]
pub struct JvString {
    inner: Rc<String>,
}

impl JvString {
    /// Create a new string
    pub fn new(s: String) -> Self {
        JvString { inner: Rc::new(s) }
    }

    /// Create from a string slice
    pub fn from_slice(s: &str) -> Self {
        JvString::new(s.to_string())
    }

    /// Get the string as a slice
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Get the length in bytes
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the length in Unicode codepoints (what jq's `length` returns for strings)
    pub fn char_len(&self) -> usize {
        self.inner.chars().count()
    }

    /// Concatenate two strings
    pub fn concat(&self, other: &JvString) -> JvString {
        let mut s = String::with_capacity(self.len() + other.len());
        s.push_str(&self.inner);
        s.push_str(&other.inner);
        JvString::new(s)
    }

    /// Get a substring by character indices
    pub fn slice(&self, start: usize, end: usize) -> JvString {
        let chars: Vec<char> = self.inner.chars().collect();
        let start = start.min(chars.len());
        let end = end.min(chars.len());
        if start >= end {
            return JvString::new(String::new());
        }
        let s: String = chars[start..end].iter().collect();
        JvString::new(s)
    }

    /// Convert to uppercase
    pub fn to_uppercase(&self) -> JvString {
        JvString::new(self.inner.to_uppercase())
    }

    /// Convert to lowercase
    pub fn to_lowercase(&self) -> JvString {
        JvString::new(self.inner.to_lowercase())
    }

    /// Check if string starts with prefix
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.inner.starts_with(prefix)
    }

    /// Check if string ends with suffix
    pub fn ends_with(&self, suffix: &str) -> bool {
        self.inner.ends_with(suffix)
    }

    /// Check if string contains substring
    pub fn contains(&self, pattern: &str) -> bool {
        self.inner.contains(pattern)
    }

    /// Split string by separator
    pub fn split(&self, sep: &str) -> Vec<JvString> {
        self.inner.split(sep).map(JvString::from_slice).collect()
    }

    /// Trim whitespace from both ends
    pub fn trim(&self) -> JvString {
        JvString::from_slice(self.inner.trim())
    }

    /// Remove prefix if present
    pub fn ltrimstr(&self, prefix: &str) -> JvString {
        if self.inner.starts_with(prefix) {
            JvString::from_slice(&self.inner[prefix.len()..])
        } else {
            self.clone()
        }
    }

    /// Remove suffix if present
    pub fn rtrimstr(&self, suffix: &str) -> JvString {
        if self.inner.ends_with(suffix) {
            JvString::from_slice(&self.inner[..self.inner.len() - suffix.len()])
        } else {
            self.clone()
        }
    }

    /// Get the underlying String (clones if shared)
    pub fn into_string(self) -> String {
        Rc::try_unwrap(self.inner).unwrap_or_else(|rc| (*rc).clone())
    }
}

impl Deref for JvString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PartialEq for JvString {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for JvString {}

impl PartialOrd for JvString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JvString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl Hash for JvString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl fmt::Display for JvString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<&str> for JvString {
    fn from(s: &str) -> Self {
        JvString::from_slice(s)
    }
}

impl From<String> for JvString {
    fn from(s: String) -> Self {
        JvString::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let s = JvString::new("hello".to_string());
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
        assert_eq!(s.char_len(), 5);
    }

    #[test]
    fn test_unicode_length() {
        let s = JvString::new("héllo".to_string());
        assert_eq!(s.char_len(), 5);
        assert!(s.len() > 5); // UTF-8 bytes > chars
    }

    #[test]
    fn test_concat() {
        let a = JvString::new("hello".to_string());
        let b = JvString::new(" world".to_string());
        assert_eq!(a.concat(&b).as_str(), "hello world");
    }

    #[test]
    fn test_slice() {
        let s = JvString::new("hello".to_string());
        assert_eq!(s.slice(1, 4).as_str(), "ell");
    }

    #[test]
    fn test_case() {
        let s = JvString::new("Hello".to_string());
        assert_eq!(s.to_uppercase().as_str(), "HELLO");
        assert_eq!(s.to_lowercase().as_str(), "hello");
    }

    #[test]
    fn test_trim() {
        let s = JvString::new("prefix_hello".to_string());
        assert_eq!(s.ltrimstr("prefix_").as_str(), "hello");

        let s = JvString::new("hello_suffix".to_string());
        assert_eq!(s.rtrimstr("_suffix").as_str(), "hello");
    }
}
