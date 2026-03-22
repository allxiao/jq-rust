//! String interning for common object keys
//!
//! Reduces memory usage and allocation overhead for frequently used keys.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

thread_local! {
    static INTERNER: RefCell<StringInterner> = RefCell::new(StringInterner::new());
}

/// A string interner that caches frequently used strings
struct StringInterner {
    strings: HashMap<String, Rc<str>>,
}

impl StringInterner {
    fn new() -> Self {
        StringInterner {
            strings: HashMap::with_capacity(64),
        }
    }

    fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(rc) = self.strings.get(s) {
            return rc.clone();
        }

        let rc: Rc<str> = s.into();
        self.strings.insert(s.to_string(), rc.clone());
        rc
    }
}

/// Intern a string, returning a reference-counted pointer
///
/// Common strings like object keys will be deduplicated.
#[inline]
pub fn intern(s: &str) -> Rc<str> {
    INTERNER.with(|interner| interner.borrow_mut().intern(s))
}

/// Check if a string is already interned
#[inline]
pub fn is_interned(s: &str) -> bool {
    INTERNER.with(|interner| interner.borrow().strings.contains_key(s))
}

/// Get stats about the interner
pub fn interner_stats() -> (usize, usize) {
    INTERNER.with(|interner| {
        let int = interner.borrow();
        let count = int.strings.len();
        let bytes: usize = int.strings.keys().map(|k| k.len()).sum();
        (count, bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_same_string() {
        let a = intern("test");
        let b = intern("test");
        assert!(Rc::ptr_eq(&a, &b));
    }

    #[test]
    fn test_intern_different_strings() {
        let a = intern("hello");
        let b = intern("world");
        assert!(!Rc::ptr_eq(&a, &b));
    }
}
