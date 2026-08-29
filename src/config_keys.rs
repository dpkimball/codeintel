//! Extract architectural config keys from manifests into bindb.
//!
//! Payloads are dotted keys plus identifier tokens (env names, Docker flags).
//! Full extra_args strings and URLs are not stored.

use crate::model::ConfigKeyRecord;

pub fn is_dpk_toml(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p == "dpk.toml" || p.ends_with("/dpk.toml")
}

pub fn is_env_manifest(path: &str, text: &str) -> bool {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let name = p.rsplit('/').next().unwrap_or("");
    if name == "dockerfile" {
        return true;
    }
    let yaml = p.ends_with(".yaml") || p.ends_with(".yml") || p.ends_with(".tpl");
    if !yaml {
        return false;
    }
    name.starts_with("values")
        || p.contains("/templates/")
        || p.contains("/charts/")
        || text.contains("OLLAMA_HOST")
}

/// Helm values/templates and Dockerfiles: env names as keys, not blobs.
pub fn extract_env_manifest(path: &str, text: &str) -> Vec<ConfigKeyRecord> {
    if !is_env_manifest(path, text) {
        return Vec::new();
    }
    let mut tokens = env_tokens(text);
    if text.contains("OLLAMA_HOST") && !tokens.iter().any(|t| t == "OLLAMA_HOST") {
        tokens.push("OLLAMA_HOST".into());
    }
    // values-dev.yaml has `ollama.host` but not the env name string.
    if is_helm_values_file(path)
        && yaml_key_present(text, "ollama")
        && !tokens.iter().any(|t| t == "OLLAMA_HOST")
    {
        tokens.push("OLLAMA_HOST".into());
    }
    tokens.sort();
    tokens.dedup();
    tokens
        .into_iter()
        .filter(|t| t.len() >= 8)
        .map(|t| ConfigKeyRecord {
            file_path: path.to_string(),
            key: format!("env.{t}"),
            value_tokens: vec![t],
        })
        .collect()
}

pub fn is_helm_values_file(path: &str) -> bool {
    let name = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    name == "values.yaml"
        || name == "values.yml"
        || (name.starts_with("values-") && (name.ends_with(".yaml") || name.ends_with(".yml")))
}

/// Chart directory for a template or values file (`charts/foo` from `charts/foo/templates/x`).
pub fn helm_chart_dir(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    if let Some(i) = p.find("/templates/") {
        return Some(p[..i].to_string());
    }
    if is_helm_values_file(&p) {
        return Some(
            p.rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default(),
        );
    }
    None
}

fn yaml_key_present(text: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    text.lines().any(|line| {
        let t = line.trim();
        !t.starts_with('#') && (t == prefix.as_str() || t.starts_with(&prefix))
    })
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

    #[test]
    fn extracts_ollama_host_from_helm_template() {
        let text = "- name: OLLAMA_HOST\n  value: {{ .host | quote }}\n";
        let keys = extract_env_manifest("charts/templates/_helpers.tpl", text);
        assert!(keys.iter().any(|k| k.key == "env.OLLAMA_HOST"));
        assert!(!is_env_manifest("README.md", text));
        assert!(extract_env_manifest("README.md", text).is_empty());
    }

    #[test]
    fn values_overlay_without_env_string_still_gets_ollama_host() {
        let text = "ollama:\n  host: \"http://dev-proxies-ollama.dev.svc.cluster.local:11434\"\n";
        let keys = extract_env_manifest("charts/app/values-dev.yaml", text);
        assert!(keys.iter().any(|k| k.key == "env.OLLAMA_HOST"));
    }
}
