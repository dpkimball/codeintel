//! Extract architectural config keys from manifests into bindb.
//!
//! Payloads are dotted keys plus identifier tokens (env names, Docker flags).
//! Full extra_args strings and URLs are not stored.

use crate::model::ConfigKeyRecord;

pub fn is_dpk_toml(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p == "dpk.toml" || p.ends_with("/dpk.toml")
}

pub fn extract_dpk_toml(path: &str, text: &str) -> Vec<ConfigKeyRecord> {
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    flatten(path, "", &value, &mut out);
    out
}

fn flatten(file_path: &str, prefix: &str, value: &toml::Value, out: &mut Vec<ConfigKeyRecord>) {
    match value {
        toml::Value::Table(table) => {
            for (k, child) in table {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(file_path, &next, child, out);
            }
        }
        toml::Value::Array(items) => {
            let mut tokens = Vec::new();
            for item in items {
                match item {
                    toml::Value::String(s) => tokens.extend(tokens_from_str(s)),
                    other => flatten(file_path, prefix, other, out),
                }
            }
            tokens.sort();
            tokens.dedup();
            if !prefix.is_empty() {
                out.push(ConfigKeyRecord {
                    file_path: file_path.to_string(),
                    key: prefix.to_string(),
                    value_tokens: tokens,
                });
            }
        }
        toml::Value::String(s) => {
            if prefix.is_empty() {
                return;
            }
            out.push(ConfigKeyRecord {
                file_path: file_path.to_string(),
                key: prefix.to_string(),
                value_tokens: tokens_from_str(s),
            });
        }
        _ => {
            if !prefix.is_empty() {
                out.push(ConfigKeyRecord {
                    file_path: file_path.to_string(),
                    key: prefix.to_string(),
                    value_tokens: Vec::new(),
                });
            }
        }
    }
}

fn tokens_from_str(s: &str) -> Vec<String> {
    let mut tokens = env_tokens(s);
    for word in s.split_whitespace() {
        let word = word.trim_matches(|c: char| c == ',' || c == '"' || c == '\'');
        if word.starts_with("--") && word.len() >= 8 {
            tokens.push(word.to_string());
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn env_tokens(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_')
            {
                i += 1;
            }
            if i - start >= 8 {
                out.push(s[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_docker_extra_args_tokens() {
        let text = r#"
[project]
name = "binder-server"
[docker]
image_name = "binder-server"
extra_args = "--build-arg CARGO_REGISTRIES_DPK_INDEX=sparse+http://host.docker.internal:31127/api/v1/crates/"
"#;
        let keys = extract_dpk_toml("dpk.toml", text);
        let extra = keys.iter().find(|k| k.key == "docker.extra_args").unwrap();
        assert!(extra
            .value_tokens
            .iter()
            .any(|t| t == "CARGO_REGISTRIES_DPK_INDEX"));
        assert!(extra.value_tokens.iter().any(|t| t == "--build-arg"));
        assert!(!extra
            .value_tokens
            .iter()
            .any(|t| t.contains("http://") || t == "binder-server"));
    }
}
