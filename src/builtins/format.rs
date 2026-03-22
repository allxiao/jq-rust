//! Format functions for jq (@base64, @uri, @csv, etc.)

use crate::jv::Jv;

/// Base64 encode a string
pub fn base64_encode(s: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(s.as_bytes())
}

/// Base64 decode a string
pub fn base64_decode(s: &str) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(s).map_err(|e| format!("base64 decode error: {}", e))?;
    String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 in base64: {}", e))
}

/// URI encode a string (percent encoding)
pub fn uri_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// URI decode a string (percent decoding)
pub fn uri_decode(s: &str) -> Result<String, String> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() != 2 {
                return Err("incomplete percent encoding".to_string());
            }
            let byte = u8::from_str_radix(&hex, 16)
                .map_err(|_| format!("invalid hex in percent encoding: %{}", hex))?;
            result.push(byte);
        } else if c == '+' {
            result.push(b' ');
        } else {
            for byte in c.to_string().as_bytes() {
                result.push(*byte);
            }
        }
    }

    String::from_utf8(result).map_err(|e| format!("invalid UTF-8: {}", e))
}

/// Escape HTML special characters
pub fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

/// Convert value to CSV field
pub fn csv_field(v: &Jv) -> String {
    match v {
        Jv::Null => String::new(),
        Jv::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        Jv::Number(n) => format!("{}", n),
        Jv::String(s) => {
            let s = s.as_str();
            if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        }
        _ => format!("{}", v),
    }
}

/// Convert array to CSV row
pub fn to_csv(arr: &[Jv]) -> String {
    arr.iter().map(csv_field).collect::<Vec<_>>().join(",")
}

/// Convert value to TSV field
pub fn tsv_field(v: &Jv) -> String {
    match v {
        Jv::Null => String::new(),
        Jv::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        Jv::Number(n) => format!("{}", n),
        Jv::String(s) => {
            s.as_str()
                .replace('\\', "\\\\")
                .replace('\t', "\\t")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        }
        _ => format!("{}", v),
    }
}

/// Convert array to TSV row
pub fn to_tsv(arr: &[Jv]) -> String {
    arr.iter().map(tsv_field).collect::<Vec<_>>().join("\t")
}

/// Escape string for shell
pub fn sh_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }

    // Check if string needs quoting
    let needs_quoting = s.chars().any(|c| {
        !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '.' | '/' | ':' | '@')
    });

    if !needs_quoting {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Convert Jv to JSON string
pub fn to_json(v: &Jv) -> String {
    use crate::jv::print_jv;
    print_jv(v)
}

/// Convert Jv to text (for @text format)
pub fn to_text(v: &Jv) -> String {
    match v {
        Jv::String(s) => s.as_str().to_string(),
        Jv::Null => String::new(),
        _ => to_json(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64() {
        assert_eq!(base64_encode("hello"), "aGVsbG8=");
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), "hello");
    }

    #[test]
    fn test_uri() {
        assert_eq!(uri_encode("hello world"), "hello%20world");
        assert_eq!(uri_encode("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(uri_decode("hello%20world").unwrap(), "hello world");
    }

    #[test]
    fn test_html() {
        assert_eq!(html_escape("<div>"), "&lt;div&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn test_csv() {
        assert_eq!(csv_field(&Jv::string("hello")), "hello");
        assert_eq!(csv_field(&Jv::string("hello,world")), "\"hello,world\"");
        assert_eq!(csv_field(&Jv::string("say \"hi\"")), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_sh() {
        assert_eq!(sh_escape("hello"), "hello");
        assert_eq!(sh_escape("hello world"), "'hello world'");
        assert_eq!(sh_escape("it's"), "'it'\\''s'");
    }
}
