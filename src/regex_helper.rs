//! Regex helper module
//!
//! Provides utilities for regex handling that supports both standard patterns
//! and patterns requiring lookahead/lookbehind (via fancy-regex).

/// Check if a pattern requires fancy-regex (contains lookahead/lookbehind)
pub fn needs_fancy_regex(pattern: &str) -> bool {
    // Lookahead: (?=...), (?!...)
    // Lookbehind: (?<=...), (?<!...)
    pattern.contains("(?=")
        || pattern.contains("(?!")
        || pattern.contains("(?<=")
        || pattern.contains("(?<!")
}

/// Compile a regex, using fancy-regex if needed
pub fn compile_regex(pattern: &str) -> Result<CompiledRegex, String> {
    if needs_fancy_regex(pattern) {
        fancy_regex::Regex::new(pattern)
            .map(CompiledRegex::Fancy)
            .map_err(|e| format!("invalid regex: {}", e))
    } else {
        regex::Regex::new(pattern)
            .map(CompiledRegex::Standard)
            .map_err(|e| format!("invalid regex: {}", e))
    }
}

/// A compiled regex that can be either standard or fancy
pub enum CompiledRegex {
    Standard(regex::Regex),
    Fancy(fancy_regex::Regex),
}

impl CompiledRegex {
    /// Check if the pattern matches anywhere in the text
    pub fn is_match(&self, text: &str) -> Result<bool, String> {
        match self {
            CompiledRegex::Standard(re) => Ok(re.is_match(text)),
            CompiledRegex::Fancy(re) => {
                re.is_match(text).map_err(|e| format!("regex error: {}", e))
            }
        }
    }

    /// Find the first match in the text
    pub fn find(&self, text: &str) -> Result<Option<MatchInfo>, String> {
        match self {
            CompiledRegex::Standard(re) => Ok(re.find(text).map(|m| MatchInfo {
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
            })),
            CompiledRegex::Fancy(re) => re
                .find(text)
                .map_err(|e| format!("regex error: {}", e))
                .map(|opt| {
                    opt.map(|m| MatchInfo {
                        start: m.start(),
                        end: m.end(),
                        text: m.as_str().to_string(),
                    })
                }),
        }
    }

    /// Find all matches in the text - returns collected results
    pub fn find_all(&self, text: &str) -> Result<Vec<MatchInfo>, String> {
        match self {
            CompiledRegex::Standard(re) => Ok(re
                .find_iter(text)
                .map(|m| MatchInfo {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                })
                .collect()),
            CompiledRegex::Fancy(re) => {
                let mut results = Vec::new();
                for m in re.find_iter(text) {
                    let m = m.map_err(|e| format!("regex error: {}", e))?;
                    results.push(MatchInfo {
                        start: m.start(),
                        end: m.end(),
                        text: m.as_str().to_string(),
                    });
                }
                Ok(results)
            }
        }
    }

    /// Get capture groups for a match - returns collected results
    pub fn captures(&self, text: &str) -> Result<Option<CaptureInfo>, String> {
        match self {
            CompiledRegex::Standard(re) => Ok(re.captures(text).map(|caps| {
                let groups: Vec<_> = caps
                    .iter()
                    .map(|opt| {
                        opt.map(|m| MatchInfo {
                            start: m.start(),
                            end: m.end(),
                            text: m.as_str().to_string(),
                        })
                    })
                    .collect();
                CaptureInfo { groups }
            })),
            CompiledRegex::Fancy(re) => re
                .captures(text)
                .map_err(|e| format!("regex error: {}", e))
                .map(|opt| {
                    opt.map(|caps| {
                        let groups: Vec<_> = caps
                            .iter()
                            .map(|opt| {
                                opt.map(|m| MatchInfo {
                                    start: m.start(),
                                    end: m.end(),
                                    text: m.as_str().to_string(),
                                })
                            })
                            .collect();
                        CaptureInfo { groups }
                    })
                }),
        }
    }

    /// Get all captures in the text
    pub fn captures_all(&self, text: &str) -> Result<Vec<CaptureInfo>, String> {
        match self {
            CompiledRegex::Standard(re) => Ok(re
                .captures_iter(text)
                .map(|caps| {
                    let groups: Vec<_> = caps
                        .iter()
                        .map(|opt| {
                            opt.map(|m| MatchInfo {
                                start: m.start(),
                                end: m.end(),
                                text: m.as_str().to_string(),
                            })
                        })
                        .collect();
                    CaptureInfo { groups }
                })
                .collect()),
            CompiledRegex::Fancy(re) => {
                let mut results = Vec::new();
                for caps_result in re.captures_iter(text) {
                    let caps = caps_result.map_err(|e| format!("regex error: {}", e))?;
                    let groups: Vec<_> = caps
                        .iter()
                        .map(|opt| {
                            opt.map(|m| MatchInfo {
                                start: m.start(),
                                end: m.end(),
                                text: m.as_str().to_string(),
                            })
                        })
                        .collect();
                    results.push(CaptureInfo { groups });
                }
                Ok(results)
            }
        }
    }

    /// Get the capture group names
    pub fn capture_names(&self) -> Vec<Option<String>> {
        match self {
            CompiledRegex::Standard(re) => {
                re.capture_names().map(|n| n.map(String::from)).collect()
            }
            CompiledRegex::Fancy(re) => re.capture_names().map(|n| n.map(String::from)).collect(),
        }
    }

    /// Replace the first match
    pub fn replace(&self, text: &str, replacement: &str) -> String {
        match self {
            CompiledRegex::Standard(re) => re.replace(text, replacement).into_owned(),
            CompiledRegex::Fancy(re) => re.replace(text, replacement).into_owned(),
        }
    }

    /// Replace all matches
    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        match self {
            CompiledRegex::Standard(re) => re.replace_all(text, replacement).into_owned(),
            CompiledRegex::Fancy(re) => re.replace_all(text, replacement).into_owned(),
        }
    }
}

/// Information about a match
#[derive(Debug, Clone)]
pub struct MatchInfo {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Information about captures
#[derive(Debug, Clone)]
pub struct CaptureInfo {
    pub groups: Vec<Option<MatchInfo>>,
}

impl CaptureInfo {
    /// Get a capture group by index
    pub fn get(&self, index: usize) -> Option<&MatchInfo> {
        self.groups.get(index).and_then(|opt| opt.as_ref())
    }

    /// Number of capture groups
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_regex() {
        let re = compile_regex(r"\d+").unwrap();
        assert!(re.is_match("abc123def").unwrap());

        let m = re.find("abc123def").unwrap().unwrap();
        assert_eq!(m.text, "123");
        assert_eq!(m.start, 3);
        assert_eq!(m.end, 6);
    }

    #[test]
    fn test_lookahead_regex() {
        let re = compile_regex(r"(?=u)").unwrap();
        assert!(re.is_match("quux").unwrap());

        let matches = re.find_all("quux").unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start, 1);
        assert_eq!(matches[1].start, 2);
    }

    #[test]
    fn test_lookbehind_regex() {
        let re = compile_regex(r"(?<=q)u").unwrap();
        assert!(re.is_match("quux").unwrap());

        let m = re.find("quux").unwrap().unwrap();
        assert_eq!(m.text, "u");
        assert_eq!(m.start, 1);
    }

    #[test]
    fn test_needs_fancy() {
        assert!(!needs_fancy_regex(r"\d+"));
        assert!(!needs_fancy_regex(r"[a-z]+"));
        assert!(needs_fancy_regex(r"(?=u)"));
        assert!(needs_fancy_regex(r"(?!u)"));
        assert!(needs_fancy_regex(r"(?<=q)u"));
        assert!(needs_fancy_regex(r"(?<!q)u"));
    }
}
