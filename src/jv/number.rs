//! Number type for JV
//!
//! Handles both integer and floating-point numbers.
//! jq uses IEEE 754 doubles internally but tries to preserve integer precision.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// JSON number representation
///
/// We store numbers that can be exactly represented as i64 in their integer form,
/// and all others as f64. This preserves integer precision for common cases.
#[derive(Debug, Clone, Copy)]
pub struct JvNumber {
    value: f64,
    /// Cached integer value if this is an exact integer
    int_value: Option<i64>,
}

impl JvNumber {
    /// Maximum integer that can be exactly represented in f64
    /// 2^53 = 9007199254740992
    const MAX_EXACT_INT: i64 = (1i64 << 53);

    /// Create a number from an i64
    pub fn from_i64(n: i64) -> Self {
        let value = n as f64;
        // Only preserve integer value if it can be exactly represented in f64
        let int_value = if n.abs() <= Self::MAX_EXACT_INT {
            Some(n)
        } else {
            // For large integers, check if round-trip conversion preserves value
            let back = value as i64;
            if back == n {
                Some(n)
            } else {
                None
            }
        };
        JvNumber { value, int_value }
    }

    /// Create a number from an f64
    pub fn from_f64(n: f64) -> Self {
        // Check if this float is actually an integer
        let int_value = if n.is_finite() && n.fract() == 0.0 {
            let i = n as i64;
            // Verify round-trip conversion
            if i as f64 == n {
                Some(i)
            } else {
                None
            }
        } else {
            None
        };

        JvNumber {
            value: n,
            int_value,
        }
    }

    /// Get the value as f64
    #[inline]
    pub fn as_f64(&self) -> f64 {
        self.value
    }

    /// Get the value as i64 if it's an exact integer
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        self.int_value
    }

    /// Check if this number is an integer
    #[inline]
    pub fn is_integer(&self) -> bool {
        self.int_value.is_some()
    }

    /// Check if the number is NaN
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.value.is_nan()
    }

    /// Check if the number is infinite
    #[inline]
    pub fn is_infinite(&self) -> bool {
        self.value.is_infinite()
    }

    /// Check if the number is finite
    #[inline]
    pub fn is_finite(&self) -> bool {
        self.value.is_finite()
    }

    // ========== Arithmetic operations ==========

    pub fn add(&self, other: &JvNumber) -> JvNumber {
        match (self.int_value, other.int_value) {
            (Some(a), Some(b)) => {
                // Try integer addition first to preserve precision
                if let Some(result) = a.checked_add(b) {
                    return JvNumber::from_i64(result);
                }
            }
            _ => {}
        }
        JvNumber::from_f64(self.value + other.value)
    }

    pub fn sub(&self, other: &JvNumber) -> JvNumber {
        match (self.int_value, other.int_value) {
            (Some(a), Some(b)) => {
                if let Some(result) = a.checked_sub(b) {
                    return JvNumber::from_i64(result);
                }
            }
            _ => {}
        }
        JvNumber::from_f64(self.value - other.value)
    }

    pub fn mul(&self, other: &JvNumber) -> JvNumber {
        match (self.int_value, other.int_value) {
            (Some(a), Some(b)) => {
                if let Some(result) = a.checked_mul(b) {
                    return JvNumber::from_i64(result);
                }
            }
            _ => {}
        }
        JvNumber::from_f64(self.value * other.value)
    }

    pub fn div(&self, other: &JvNumber) -> JvNumber {
        // Division always produces float in jq
        JvNumber::from_f64(self.value / other.value)
    }

    pub fn modulo(&self, other: &JvNumber) -> JvNumber {
        // jq returns NaN if either operand is NaN
        if self.value.is_nan() || other.value.is_nan() {
            return JvNumber::from_f64(f64::NAN);
        }

        // jq uses integer modulo semantics - converts both operands to intmax_t first
        // This matches jq's dtoi macro behavior:
        // #define dtoi(n) ((n) < INTMAX_MIN ? INTMAX_MIN : -(n) <= INTMAX_MIN ? INTMAX_MAX : (intmax_t)(n))
        fn dtoi(n: f64) -> i64 {
            if n < i64::MIN as f64 {
                return i64::MIN;
            }
            if (-n) <= i64::MIN as f64 {
                return i64::MAX;
            }
            n as i64
        }

        let a = dtoi(self.value);
        let b = dtoi(other.value);

        if b == 0 {
            return JvNumber::from_f64(f64::NAN);
        }
        if b == -1 {
            // Avoid overflow when a is i64::MIN
            return JvNumber::from_i64(0);
        }
        JvNumber::from_i64(a % b)
    }

    pub fn neg(&self) -> JvNumber {
        match self.int_value {
            Some(n) => {
                if let Some(result) = n.checked_neg() {
                    return JvNumber::from_i64(result);
                }
            }
            None => {}
        }
        JvNumber::from_f64(-self.value)
    }

    pub fn floor(&self) -> JvNumber {
        JvNumber::from_f64(self.value.floor())
    }

    pub fn ceil(&self) -> JvNumber {
        JvNumber::from_f64(self.value.ceil())
    }

    pub fn round(&self) -> JvNumber {
        JvNumber::from_f64(self.value.round())
    }

    pub fn abs(&self) -> JvNumber {
        match self.int_value {
            Some(n) => {
                if let Some(result) = n.checked_abs() {
                    return JvNumber::from_i64(result);
                }
            }
            None => {}
        }
        JvNumber::from_f64(self.value.abs())
    }

    pub fn sqrt(&self) -> JvNumber {
        JvNumber::from_f64(self.value.sqrt())
    }
}

impl PartialEq for JvNumber {
    fn eq(&self, other: &Self) -> bool {
        // Handle NaN: NaN != NaN in IEEE 754
        if self.value.is_nan() && other.value.is_nan() {
            return false;
        }
        self.value == other.value
    }
}

impl Eq for JvNumber {}

impl PartialOrd for JvNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for JvNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        // For sorting purposes, we need a total order
        // NaN is considered greater than all other values
        self.value.partial_cmp(&other.value).unwrap_or_else(|| {
            match (self.value.is_nan(), other.value.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => unreachable!(),
            }
        })
    }
}

impl Hash for JvNumber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the bits of the f64 for consistent hashing
        // This ensures that equal values hash equally
        if let Some(i) = self.int_value {
            i.hash(state);
        } else {
            self.value.to_bits().hash(state);
        }
    }
}

impl fmt::Display for JvNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(i) = self.int_value {
            write!(f, "{}", i)
        } else if self.value.is_nan() {
            write!(f, "null") // jq outputs null for NaN
        } else if self.value.is_infinite() {
            // jq outputs very large numbers for infinity
            if self.value.is_sign_positive() {
                write!(f, "1.7976931348623157e+308")
            } else {
                write!(f, "-1.7976931348623157e+308")
            }
        } else {
            // Use jq-compatible number formatting
            // Avoid unnecessary decimal places for whole numbers
            let formatted = format!("{}", self.value);
            // jq doesn't use '+' in exponents
            write!(f, "{}", formatted.replace("e+", "e"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_preservation() {
        let n = JvNumber::from_i64(42);
        assert_eq!(n.as_i64(), Some(42));
        assert_eq!(n.as_f64(), 42.0);
        assert!(n.is_integer());
    }

    #[test]
    fn test_float_to_int() {
        let n = JvNumber::from_f64(42.0);
        assert_eq!(n.as_i64(), Some(42));
        assert!(n.is_integer());

        let n = JvNumber::from_f64(42.5);
        assert_eq!(n.as_i64(), None);
        assert!(!n.is_integer());
    }

    #[test]
    fn test_arithmetic() {
        let a = JvNumber::from_i64(10);
        let b = JvNumber::from_i64(3);

        assert_eq!(a.add(&b).as_i64(), Some(13));
        assert_eq!(a.sub(&b).as_i64(), Some(7));
        assert_eq!(a.mul(&b).as_i64(), Some(30));
        // Division always returns float
        assert!(!a.div(&b).is_integer());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", JvNumber::from_i64(42)), "42");
        assert_eq!(format!("{}", JvNumber::from_f64(3.14)), "3.14");
    }
}
