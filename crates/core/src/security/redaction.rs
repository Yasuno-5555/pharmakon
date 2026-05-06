use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref RE_API_KEY: Regex = Regex::new(
        r"(?i)(api[-_]?key|secret|token|password|auth|credential)[:\s=]+[a-zA-Z0-9\-_]{8,}"
    )
    .unwrap();
    static ref RE_EMAIL: Regex =
        Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    static ref RE_CREDIT_CARD: Regex = Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap();
}

pub struct Redactor {
    patterns: Vec<(Regex, String)>,
}

impl Redactor {
    pub fn new() -> Self {
        let mut patterns = Vec::new();
        patterns.push((RE_API_KEY.clone(), "[REDACTED_API_KEY]".to_string()));
        patterns.push((RE_EMAIL.clone(), "[REDACTED_EMAIL]".to_string()));
        patterns.push((RE_CREDIT_CARD.clone(), "[REDACTED_CC]".to_string()));
        Self { patterns }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (re, replacement) in &self.patterns {
            result = re.replace_all(&result, replacement).to_string();
        }
        result
    }
}

pub fn redact_text(text: &str) -> String {
    let redactor = Redactor::new();
    redactor.redact(text)
}
