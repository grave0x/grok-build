//! Secret redaction for audit entries.
//!
//! Strips API keys, passwords, and tokens from tool params before storage.

use regex::Regex;

/// Redacts secrets from JSON values using configurable patterns.
pub struct SecretRedactor {
    patterns: Vec<Regex>,
}

impl Default for SecretRedactor {
    fn default() -> Self {
        Self {
            patterns: vec![
                Regex::new(r#"xai-[a-zA-Z0-9_-]{20,}"#).unwrap(),
                Regex::new(r#"sk-[a-zA-Z0-9_-]{20,}"#).unwrap(),
                Regex::new(r#"Bearer\s+[a-zA-Z0-9._\-]+"#).unwrap(),
            ],
        }
    }
}

impl SecretRedactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Redact secrets from a JSON value recursively.
    pub fn redact_json(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let redacted: serde_json::Map<String, serde_json::Value> = map
                    .into_iter()
                    .map(|(k, v)| {
                        let v = match &v {
                            serde_json::Value::String(s) => {
                                serde_json::Value::String(self.redact_str(s))
                            }
                            other => self.redact_json(other.clone()),
                        };
                        (k, v)
                    })
                    .collect();
                serde_json::Value::Object(redacted)
            }
            serde_json::Value::String(s) => {
                serde_json::Value::String(self.redact_str(&s))
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(
                    arr.into_iter()
                        .map(|v| self.redact_json(v))
                        .collect(),
                )
            }
            other => other,
        }
    }

    fn redact_str(&self, s: &str) -> String {
        let mut result = s.to_string();
        for pattern in &self.patterns {
            if pattern.is_match(&result) {
                result = pattern.replace_all(&result, "***REDACTED***").to_string();
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_api_key() {
        let redactor = SecretRedactor::default();
        let input = serde_json::json!({
            "command": "curl -H 'Authorization: Bearer xai-abc123def45678901234567' https://api.example.com",
            "api_key": "sk-mysecretkey1234567890"
        });
        let output = redactor.redact_json(input);
        let s = output.to_string();
        assert!(!s.contains("xai-abc123def45678901234567"));
        assert!(!s.contains("sk-mysecretkey1234567890"));
        assert!(s.contains("***REDACTED***"));
    }
}
