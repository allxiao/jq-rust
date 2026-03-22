//! Core JV (JSON Value) type definition
//!
//! This is the central data type for jq, representing any JSON value.

use std::fmt;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use super::{JvNumber, JvString, JvArray, JvObject};
use crate::error::JqError;

/// The core JSON value type
///
/// Equivalent to `jv` in the C implementation. Unlike C where jv uses
/// a tagged union with manual reference counting, we use Rust enums
/// with interior reference counting via Rc/Arc where needed.
#[derive(Debug, Clone)]
pub enum Jv {
    /// JSON null
    Null,
    /// JSON boolean
    Bool(bool),
    /// JSON number (stored as f64, with special handling for integers)
    Number(JvNumber),
    /// JSON string
    String(JvString),
    /// JSON array
    Array(JvArray),
    /// JSON object
    Object(JvObject),
    /// Invalid value (used for error propagation)
    /// The optional error message provides context
    Invalid(Option<Box<JqError>>),
}

impl Jv {
    // ========== Constructors ==========

    /// Create a null value
    #[inline]
    pub fn null() -> Self {
        Jv::Null
    }

    /// Create a boolean value
    #[inline]
    pub fn bool(b: bool) -> Self {
        Jv::Bool(b)
    }

    /// Create a number from an integer
    #[inline]
    pub fn from_i64(n: i64) -> Self {
        Jv::Number(JvNumber::from_i64(n))
    }

    /// Create a number from a float
    #[inline]
    pub fn from_f64(n: f64) -> Self {
        Jv::Number(JvNumber::from_f64(n))
    }

    /// Create a string value
    #[inline]
    pub fn string<S: Into<String>>(s: S) -> Self {
        Jv::String(JvString::new(s.into()))
    }

    /// Create an empty array
    #[inline]
    pub fn array() -> Self {
        Jv::Array(JvArray::new())
    }

    /// Create an array from a vector of values
    #[inline]
    pub fn from_vec(v: Vec<Jv>) -> Self {
        Jv::Array(JvArray::from_vec(v))
    }

    /// Create an empty object
    #[inline]
    pub fn object() -> Self {
        Jv::Object(JvObject::new())
    }

    /// Create an invalid value with an error
    #[inline]
    pub fn invalid() -> Self {
        Jv::Invalid(None)
    }

    /// Create an invalid value with an error message
    #[inline]
    pub fn invalid_with_error(err: JqError) -> Self {
        Jv::Invalid(Some(Box::new(err)))
    }

    // ========== Type checking ==========

    /// Get the type name as a string (matches jq's `type` builtin)
    pub fn type_name(&self) -> &'static str {
        match self {
            Jv::Null => "null",
            Jv::Bool(_) => "boolean",
            Jv::Number(_) => "number",
            Jv::String(_) => "string",
            Jv::Array(_) => "array",
            Jv::Object(_) => "object",
            Jv::Invalid(_) => "invalid",
        }
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, Jv::Null)
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        matches!(self, Jv::Bool(_))
    }

    #[inline]
    pub fn is_number(&self) -> bool {
        matches!(self, Jv::Number(_))
    }

    #[inline]
    pub fn is_string(&self) -> bool {
        matches!(self, Jv::String(_))
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        matches!(self, Jv::Array(_))
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        matches!(self, Jv::Object(_))
    }

    #[inline]
    pub fn is_invalid(&self) -> bool {
        matches!(self, Jv::Invalid(_))
    }

    /// Check if value is "truthy" (not false and not null)
    /// In jq, only false and null are considered falsy
    #[inline]
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Jv::Bool(false) | Jv::Null)
    }

    // ========== Value extraction ==========

    /// Get as boolean, returning None if not a boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Jv::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as number, returning None if not a number
    pub fn as_number(&self) -> Option<&JvNumber> {
        match self {
            Jv::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Get as f64, returning None if not a number
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Jv::Number(n) => Some(n.as_f64()),
            _ => None,
        }
    }

    /// Get as i64 if the number is an integer, returning None otherwise
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Jv::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    /// Get as string slice, returning None if not a string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Jv::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Get as array, returning None if not an array
    pub fn as_array(&self) -> Option<&JvArray> {
        match self {
            Jv::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Get as mutable array, returning None if not an array
    pub fn as_array_mut(&mut self) -> Option<&mut JvArray> {
        match self {
            Jv::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Get as object, returning None if not an object
    pub fn as_object(&self) -> Option<&JvObject> {
        match self {
            Jv::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Get as mutable object, returning None if not an object
    pub fn as_object_mut(&mut self) -> Option<&mut JvObject> {
        match self {
            Jv::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Get the error from an invalid value
    pub fn get_error(&self) -> Option<&JqError> {
        match self {
            Jv::Invalid(Some(e)) => Some(e),
            _ => None,
        }
    }

    // ========== Collection operations ==========

    /// Get the length of a string, array, object, or null
    pub fn len(&self) -> Option<usize> {
        match self {
            Jv::Null => Some(0),
            Jv::String(s) => Some(s.len()),
            Jv::Array(a) => Some(a.len()),
            Jv::Object(o) => Some(o.len()),
            _ => None,
        }
    }

    /// Check if empty (for strings, arrays, objects)
    pub fn is_empty(&self) -> Option<bool> {
        self.len().map(|l| l == 0)
    }

    /// Index into an array or object
    /// For arrays: index by integer
    /// For objects: index by string
    pub fn index(&self, idx: &Jv) -> Jv {
        match (self, idx) {
            (Jv::Array(arr), Jv::Number(n)) => {
                // NaN index returns null
                if n.is_nan() {
                    return Jv::null();
                }
                // jq truncates float indices to integers using floor
                let i = if let Some(i) = n.as_i64() {
                    i
                } else {
                    n.as_f64().floor() as i64
                };
                arr.get(i).unwrap_or_else(Jv::null)
            }
            (Jv::Object(obj), Jv::String(s)) => {
                obj.get(s.as_str()).unwrap_or_else(Jv::null)
            }
            (Jv::Null, _) => Jv::null(),
            (Jv::Invalid(e), _) => Jv::Invalid(e.clone()),
            (_, Jv::Invalid(e)) => Jv::Invalid(e.clone()),
            _ => Jv::invalid_with_error(JqError::Type(format!(
                "cannot index {} with {}",
                self.type_name(),
                idx.type_name()
            ))),
        }
    }

    /// Get a field from an object by name
    pub fn get_field(&self, field: &str) -> Jv {
        match self {
            Jv::Object(obj) => obj.get(field).unwrap_or_else(Jv::null),
            Jv::Null => Jv::null(),
            Jv::Invalid(e) => Jv::Invalid(e.clone()),
            _ => Jv::invalid_with_error(JqError::Type(format!(
                "cannot get field .{} from {}",
                field,
                self.type_name()
            ))),
        }
    }

    /// Iterate over values (for arrays and objects)
    pub fn iter_values(&self) -> Box<dyn Iterator<Item = Jv> + '_> {
        match self {
            Jv::Array(arr) => Box::new(arr.iter()),
            Jv::Object(obj) => Box::new(obj.values()),
            Jv::Null => Box::new(std::iter::empty()),
            _ => Box::new(std::iter::once(Jv::invalid_with_error(
                JqError::Type(format!("cannot iterate over {}", self.type_name()))
            ))),
        }
    }

    /// Iterate over key-value pairs (for objects) or index-value pairs (for arrays)
    pub fn iter_entries(&self) -> Box<dyn Iterator<Item = (Jv, Jv)> + '_> {
        match self {
            Jv::Array(arr) => Box::new(
                arr.iter()
                    .enumerate()
                    .map(|(i, v)| (Jv::from_i64(i as i64), v))
            ),
            Jv::Object(obj) => Box::new(
                obj.iter()
                    .map(|(k, v)| (Jv::string(k), v))
            ),
            _ => Box::new(std::iter::empty()),
        }
    }

    // ========== Modification operations ==========

    /// Set a value at an index (for arrays) or key (for objects)
    pub fn set_index(&mut self, idx: &Jv, value: Jv) -> Result<(), JqError> {
        match (self, idx) {
            (Jv::Array(arr), Jv::Number(n)) => {
                if let Some(i) = n.as_i64() {
                    arr.set(i, value).map_err(JqError::Type)?;
                    Ok(())
                } else {
                    Err(JqError::Type("array index must be integer".to_string()))
                }
            }
            (Jv::Object(obj), Jv::String(s)) => {
                obj.set(s.as_str(), value);
                Ok(())
            }
            (s, _) => Err(JqError::Type(format!(
                "cannot index {} with {}",
                s.type_name(),
                idx.type_name()
            ))),
        }
    }
}

// ========== Trait implementations ==========

impl Default for Jv {
    fn default() -> Self {
        Jv::Null
    }
}

impl PartialEq for Jv {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Jv::Null, Jv::Null) => true,
            (Jv::Bool(a), Jv::Bool(b)) => a == b,
            (Jv::Number(a), Jv::Number(b)) => a == b,
            (Jv::String(a), Jv::String(b)) => a == b,
            (Jv::Array(a), Jv::Array(b)) => a == b,
            (Jv::Object(a), Jv::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Jv {}

impl PartialOrd for Jv {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Jv {
    fn cmp(&self, other: &Self) -> Ordering {
        // jq ordering: null < false < true < numbers < strings < arrays < objects
        fn type_order(jv: &Jv) -> u8 {
            match jv {
                Jv::Null => 0,
                Jv::Bool(false) => 1,
                Jv::Bool(true) => 2,
                Jv::Number(_) => 3,
                Jv::String(_) => 4,
                Jv::Array(_) => 5,
                Jv::Object(_) => 6,
                Jv::Invalid(_) => 7,
            }
        }

        let type_cmp = type_order(self).cmp(&type_order(other));
        if type_cmp != Ordering::Equal {
            return type_cmp;
        }

        match (self, other) {
            (Jv::Null, Jv::Null) => Ordering::Equal,
            (Jv::Bool(a), Jv::Bool(b)) => a.cmp(b),
            (Jv::Number(a), Jv::Number(b)) => a.cmp(b),
            (Jv::String(a), Jv::String(b)) => a.cmp(b),
            (Jv::Array(a), Jv::Array(b)) => a.cmp(b),
            (Jv::Object(a), Jv::Object(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }
    }
}

impl Hash for Jv {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Jv::Null => {}
            Jv::Bool(b) => b.hash(state),
            Jv::Number(n) => n.hash(state),
            Jv::String(s) => s.hash(state),
            Jv::Array(a) => a.hash(state),
            Jv::Object(o) => o.hash(state),
            Jv::Invalid(_) => {}
        }
    }
}

impl fmt::Display for Jv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Jv::Null => write!(f, "null"),
            Jv::Bool(true) => write!(f, "true"),
            Jv::Bool(false) => write!(f, "false"),
            Jv::Number(n) => write!(f, "{}", n),
            Jv::String(s) => write!(f, "\"{}\"", s.as_str().escape_default()),
            Jv::Array(a) => write!(f, "{}", a),
            Jv::Object(o) => write!(f, "{}", o),
            Jv::Invalid(Some(e)) => write!(f, "<invalid: {}>", e),
            Jv::Invalid(None) => write!(f, "<invalid>"),
        }
    }
}

// Conversion traits
impl From<bool> for Jv {
    fn from(b: bool) -> Self {
        Jv::Bool(b)
    }
}

impl From<i64> for Jv {
    fn from(n: i64) -> Self {
        Jv::from_i64(n)
    }
}

impl From<i32> for Jv {
    fn from(n: i32) -> Self {
        Jv::from_i64(n as i64)
    }
}

impl From<f64> for Jv {
    fn from(n: f64) -> Self {
        Jv::from_f64(n)
    }
}

impl From<&str> for Jv {
    fn from(s: &str) -> Self {
        Jv::string(s)
    }
}

impl From<String> for Jv {
    fn from(s: String) -> Self {
        Jv::string(s)
    }
}

impl<T: Into<Jv>> From<Vec<T>> for Jv {
    fn from(v: Vec<T>) -> Self {
        Jv::from_vec(v.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Jv>> From<Option<T>> for Jv {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => Jv::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_names() {
        assert_eq!(Jv::null().type_name(), "null");
        assert_eq!(Jv::bool(true).type_name(), "boolean");
        assert_eq!(Jv::from_i64(42).type_name(), "number");
        assert_eq!(Jv::string("hello").type_name(), "string");
        assert_eq!(Jv::array().type_name(), "array");
        assert_eq!(Jv::object().type_name(), "object");
    }

    #[test]
    fn test_truthy() {
        assert!(!Jv::Null.is_truthy());
        assert!(!Jv::Bool(false).is_truthy());
        assert!(Jv::Bool(true).is_truthy());
        assert!(Jv::from_i64(0).is_truthy());
        assert!(Jv::string("").is_truthy());
        assert!(Jv::array().is_truthy());
    }

    #[test]
    fn test_equality() {
        assert_eq!(Jv::null(), Jv::null());
        assert_eq!(Jv::bool(true), Jv::bool(true));
        assert_ne!(Jv::bool(true), Jv::bool(false));
        assert_eq!(Jv::from_i64(42), Jv::from_i64(42));
        assert_eq!(Jv::string("hello"), Jv::string("hello"));
    }

    #[test]
    fn test_ordering() {
        // null < false < true < numbers < strings < arrays < objects
        assert!(Jv::null() < Jv::bool(false));
        assert!(Jv::bool(false) < Jv::bool(true));
        assert!(Jv::bool(true) < Jv::from_i64(0));
        assert!(Jv::from_i64(0) < Jv::string(""));
        assert!(Jv::string("z") < Jv::array());
        assert!(Jv::array() < Jv::object());
    }

    #[test]
    fn test_conversions() {
        let _: Jv = true.into();
        let _: Jv = 42i64.into();
        let _: Jv = 3.14f64.into();
        let _: Jv = "hello".into();
        let _: Jv = String::from("world").into();
        let _: Jv = vec![1i64, 2, 3].into();
    }
}
