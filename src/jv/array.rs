//! Array type for JV
//!
//! JSON arrays with copy-on-write semantics.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::Jv;

/// Maximum array index allowed (matches jq's behavior)
/// jq typically allows up to ~1 million elements before erroring
const MAX_ARRAY_INDEX: usize = 1_000_000;

/// JSON array value
///
/// Uses reference counting with copy-on-write for efficient cloning.
#[derive(Debug, Clone)]
pub struct JvArray {
    inner: Rc<RefCell<Vec<Jv>>>,
}

impl JvArray {
    /// Create a new empty array
    pub fn new() -> Self {
        JvArray {
            inner: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Create an array with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        JvArray {
            inner: Rc::new(RefCell::new(Vec::with_capacity(capacity))),
        }
    }

    /// Create an array from a vector
    pub fn from_vec(v: Vec<Jv>) -> Self {
        JvArray {
            inner: Rc::new(RefCell::new(v)),
        }
    }

    /// Get the length
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Get element at index
    /// Supports negative indexing (like Python)
    pub fn get(&self, index: i64) -> Option<Jv> {
        let arr = self.inner.borrow();
        let len = arr.len() as i64;

        let idx = if index < 0 { len + index } else { index };

        if idx >= 0 && idx < len {
            Some(arr[idx as usize].clone())
        } else {
            None
        }
    }

    /// Set element at index
    /// Supports negative indexing
    /// Extends array if necessary for positive indices
    /// Returns error if index is too large (> MAX_ARRAY_INDEX)
    pub fn set(&mut self, index: i64, value: Jv) -> Result<(), String> {
        // Ensure unique ownership before mutation
        self.make_unique();

        let mut arr = self.inner.borrow_mut();
        let len = arr.len() as i64;

        let idx = if index < 0 {
            let i = len + index;
            if i < 0 {
                return Ok(()); // Invalid negative index, jq ignores it
            }
            i as usize
        } else {
            index as usize
        };

        // Check for too large index
        if idx > MAX_ARRAY_INDEX {
            return Err("Array index too large".to_string());
        }

        // Extend array if needed
        while arr.len() <= idx {
            arr.push(Jv::Null);
        }
        arr[idx] = value;
        Ok(())
    }

    /// Push a value to the end
    pub fn push(&mut self, value: Jv) {
        self.make_unique();
        self.inner.borrow_mut().push(value);
    }

    /// Get a slice of the array
    pub fn slice(&self, start: Option<i64>, end: Option<i64>) -> JvArray {
        let arr = self.inner.borrow();
        let len = arr.len() as i64;

        let start = match start {
            Some(s) if s < 0 => (len + s).max(0) as usize,
            Some(s) => (s as usize).min(arr.len()),
            None => 0,
        };

        let end = match end {
            Some(e) if e < 0 => (len + e).max(0) as usize,
            Some(e) => (e as usize).min(arr.len()),
            None => arr.len(),
        };

        if start >= end {
            return JvArray::new();
        }

        JvArray::from_vec(arr[start..end].to_vec())
    }

    /// Concatenate two arrays
    pub fn concat(&self, other: &JvArray) -> JvArray {
        let mut result = self.inner.borrow().clone();
        result.extend(other.inner.borrow().iter().cloned());
        JvArray::from_vec(result)
    }

    /// Add an element (returns new array, jq style)
    pub fn add(&self, value: Jv) -> JvArray {
        let mut result = self.inner.borrow().clone();
        result.push(value);
        JvArray::from_vec(result)
    }

    /// Reverse the array
    pub fn reverse(&self) -> JvArray {
        let mut result = self.inner.borrow().clone();
        result.reverse();
        JvArray::from_vec(result)
    }

    /// Sort the array
    pub fn sort(&self) -> JvArray {
        let mut result = self.inner.borrow().clone();
        result.sort();
        JvArray::from_vec(result)
    }

    /// Get unique elements (preserving order)
    pub fn unique(&self) -> JvArray {
        let arr = self.inner.borrow();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for item in arr.iter() {
            // Use a simple approach: convert to string for comparison
            // This is a simplification; full implementation would use proper jv_equal
            let key = format!("{}", item);
            if seen.insert(key) {
                result.push(item.clone());
            }
        }

        JvArray::from_vec(result)
    }

    /// Flatten the array one level
    pub fn flatten(&self, depth: Option<usize>) -> JvArray {
        // None means unlimited depth (fully flatten)
        // Some(0) means no flattening
        if depth == Some(0) {
            return self.clone();
        }

        let arr = self.inner.borrow();
        let mut result = Vec::new();

        for item in arr.iter() {
            match item {
                Jv::Array(inner_arr) => {
                    // Recursively flatten with decremented depth (or None for unlimited)
                    let new_depth = depth.map(|d| d - 1);
                    let flattened = inner_arr.flatten(new_depth);
                    result.extend(flattened.inner.borrow().iter().cloned());
                }
                _ => result.push(item.clone()),
            }
        }

        JvArray::from_vec(result)
    }

    /// Iterate over elements
    pub fn iter(&self) -> impl Iterator<Item = Jv> + '_ {
        let len = self.len();
        (0..len).map(move |i| self.inner.borrow()[i].clone())
    }

    /// Check if array contains a value
    pub fn contains(&self, value: &Jv) -> bool {
        self.inner.borrow().iter().any(|v| v == value)
    }

    /// Get index of first occurrence
    pub fn index_of(&self, value: &Jv) -> Option<usize> {
        self.inner.borrow().iter().position(|v| v == value)
    }

    /// Delete element at index
    /// Returns a new array without the element
    pub fn delete(&mut self, index: i64) {
        self.make_unique();
        let mut arr = self.inner.borrow_mut();
        let len = arr.len() as i64;

        let idx = if index < 0 { len + index } else { index };

        if idx >= 0 && idx < len {
            arr.remove(idx as usize);
        }
    }

    /// Ensure unique ownership for mutation
    fn make_unique(&mut self) {
        if Rc::strong_count(&self.inner) > 1 {
            let cloned = self.inner.borrow().clone();
            self.inner = Rc::new(RefCell::new(cloned));
        }
    }

    /// Get a reference to the inner Vec (for iteration)
    pub fn as_slice(&self) -> std::cell::Ref<'_, Vec<Jv>> {
        self.inner.borrow()
    }
}

impl Default for JvArray {
    fn default() -> Self {
        JvArray::new()
    }
}

impl PartialEq for JvArray {
    fn eq(&self, other: &Self) -> bool {
        let a = self.inner.borrow();
        let b = other.inner.borrow();
        a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
    }
}

impl Eq for JvArray {}

impl PartialOrd for JvArray {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JvArray {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.inner.borrow();
        let b = other.inner.borrow();
        a.iter().cmp(b.iter())
    }
}

impl Hash for JvArray {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for item in self.inner.borrow().iter() {
            item.hash(state);
        }
    }
}

impl fmt::Display for JvArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        let arr = self.inner.borrow();
        for (i, item) in arr.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}", item)?;
        }
        write!(f, "]")
    }
}

impl FromIterator<Jv> for JvArray {
    fn from_iter<I: IntoIterator<Item = Jv>>(iter: I) -> Self {
        JvArray::from_vec(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let arr = JvArray::new();
        assert!(arr.is_empty());
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_push_and_get() {
        let mut arr = JvArray::new();
        arr.push(Jv::from_i64(1));
        arr.push(Jv::from_i64(2));
        arr.push(Jv::from_i64(3));

        assert_eq!(arr.len(), 3);
        assert_eq!(arr.get(0), Some(Jv::from_i64(1)));
        assert_eq!(arr.get(1), Some(Jv::from_i64(2)));
        assert_eq!(arr.get(-1), Some(Jv::from_i64(3))); // Negative indexing
    }

    #[test]
    fn test_slice() {
        let arr = JvArray::from_vec(vec![
            Jv::from_i64(1),
            Jv::from_i64(2),
            Jv::from_i64(3),
            Jv::from_i64(4),
        ]);

        let slice = arr.slice(Some(1), Some(3));
        assert_eq!(slice.len(), 2);
        assert_eq!(slice.get(0), Some(Jv::from_i64(2)));
        assert_eq!(slice.get(1), Some(Jv::from_i64(3)));
    }

    #[test]
    fn test_concat() {
        let a = JvArray::from_vec(vec![Jv::from_i64(1), Jv::from_i64(2)]);
        let b = JvArray::from_vec(vec![Jv::from_i64(3), Jv::from_i64(4)]);
        let c = a.concat(&b);

        assert_eq!(c.len(), 4);
    }

    #[test]
    fn test_reverse() {
        let arr = JvArray::from_vec(vec![Jv::from_i64(1), Jv::from_i64(2), Jv::from_i64(3)]);
        let rev = arr.reverse();

        assert_eq!(rev.get(0), Some(Jv::from_i64(3)));
        assert_eq!(rev.get(2), Some(Jv::from_i64(1)));
    }

    #[test]
    fn test_copy_on_write() {
        let arr1 = JvArray::from_vec(vec![Jv::from_i64(1)]);
        let mut arr2 = arr1.clone();

        // Should share the same underlying data initially
        arr2.push(Jv::from_i64(2));

        // arr1 should be unchanged
        assert_eq!(arr1.len(), 1);
        assert_eq!(arr2.len(), 2);
    }
}
