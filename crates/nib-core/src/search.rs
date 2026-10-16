//! Smart-case search specifications, parsed without compiling a regex.
/// One cancellable full-text query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSpec {
    /// Literal text or regex source.
    pub pattern: String,
    /// Use regex semantics.
    pub regex: bool,
    /// Smart-case mode.
    pub case_sensitive: bool,
    /// Path substrings to skip.
    pub excludes: Vec<String>,
}
impl SearchSpec {
    /// Parse `r:` and path exclusions while keeping ordinary spaces.
    pub fn parse(input: &str) -> Self {
        let (regex, input) = input
            .strip_prefix("r:")
            .map_or((false, input), |s| (true, s));
        let mut words = vec![];
        let mut excludes = vec![];
        for word in input.split_whitespace() {
            if let Some(x) = word.strip_prefix('-').filter(|s| !s.is_empty()) {
                excludes.push(x.into());
            } else {
                words.push(word);
            }
        }
        let pattern = words.join(" ");
        let case_sensitive = pattern.chars().any(char::is_uppercase);
        Self {
            pattern,
            regex,
            case_sensitive,
            excludes,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn specs() {
        let s = SearchSpec::parse("r:Hello.* world -daily");
        assert!(s.regex && s.case_sensitive);
        assert_eq!(s.pattern, "Hello.* world");
        assert_eq!(s.excludes, ["daily"]);
        assert!(!SearchSpec::parse("hello world").case_sensitive);
    }
}
