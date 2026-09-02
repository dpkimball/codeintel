//! Index Cursor/Claude skill files (.cursor/skills/*/SKILL.md) into bindb for semantic plan() matching.

use crate::error::{CodeintelError, Result};
use crate::ids::{skill_id, ACTOR, NAMESPACE};
use crate::indexer::put_json;
use crate::model::SkillRecord;
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::SystemTime;

/// Index all SKILL.md files found under `skills_dir` (walks one level: `*/SKILL.md`).
/// `namespace` is used as the bindb key prefix, e.g. "my-workspace".
pub fn index_skills(store: &BinderStore, skills_dir: &Path, namespace: &str) -> Result<usize> {
    store.register_namespace(NAMESPACE, ACTOR)?;

    let provenance = Provenance {
        source: ProvenanceSource::Import("skills".into()),
        source_ref: skills_dir.display().to_string(),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ..Default::default()
    };

    let entries = std::fs::read_dir(skills_dir).map_err(|e| {
        CodeintelError::Other(format!(
            "cannot read skills dir {}: {e}",
            skills_dir.display()
        ))
    })?;

    let mut count = 0;
    for entry in entries.flatten() {
        let skill_md = entry.path().join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&skill_md).map_err(|e| {
            CodeintelError::Other(format!("cannot read {}: {e}", skill_md.display()))
        })?;

        let (name, description) = parse_skill_frontmatter(&content);
        if name.is_empty() {
            continue;
        }

        let tokens = extract_skill_tokens(&name, &description);
        let rel_path = format!("{}/SKILL.md", entry.file_name().to_string_lossy());

        let rec = SkillRecord {
            name: name.clone(),
            path: rel_path,
            description,
            tokens,
            namespace: namespace.to_string(),
        };

        let mut tx = Transaction::new();
        put_json(
            &mut tx,
            skill_id(namespace, &name),
            &rec,
            Some(provenance.clone()),
        )?;
        store.commit_buffered(tx, ACTOR, format!("skill {namespace}/{name}"))?;
        count += 1;
    }

    store.flush()?;
    Ok(count)
}

/// Parse `name:` and `description:` from YAML frontmatter delimited by `---`.
fn parse_skill_frontmatter(content: &str) -> (String, String) {
    let body = content.trim_start();
    if !body.starts_with("---") {
        return (String::new(), String::new());
    }
    let rest = &body[3..];
    let end = match rest.find("\n---") {
        Some(i) => i,
        None => return (String::new(), String::new()),
    };
    let fm = &rest[..end];

    let mut name = String::new();
    let mut description = String::new();
    let mut in_description = false;

    for line in fm.lines() {
        if let Some(stripped) = line.strip_prefix("name:") {
            name = stripped.trim().trim_matches('"').to_string();
            in_description = false;
        } else if let Some(stripped) = line.strip_prefix("description:") {
            let val = stripped.trim();
            if val == ">" || val == ">-" || val == "|" {
                in_description = true;
            } else {
                description = val.trim_matches('"').to_string();
                in_description = false;
            }
        } else if in_description {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str(trimmed);
        } else {
            in_description = false;
        }
    }

    (name, description)
}

fn extract_skill_tokens(name: &str, description: &str) -> Vec<String> {
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    let text = format!("{name} {description}");
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
        let w = word.to_lowercase();
        let w = w.trim_matches(|c: char| c == '-' || c == '_');
        if w.len() >= 4 || crate::ids::SHORT_TECHNICAL_TERMS.contains(&w) {
            tokens.insert(w.to_string());
        }
    }
    tokens.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_block_scalar_description() {
        let content = r#"---
name: codeintel-query
description: >-
  Query the codeintel architecture store using the correct store path and binary.
  Use when asking about doc compliance, crate dependencies.
---
# Body
"#;
        let (name, desc) = parse_skill_frontmatter(content);
        assert_eq!(name, "codeintel-query");
        assert!(desc.contains("codeintel"), "desc: {desc}");
        assert!(desc.contains("architecture"), "desc: {desc}");
    }

    #[test]
    fn parse_inline_description() {
        let content = r#"---
name: my-skill
description: "Do the thing quickly"
---
"#;
        let (name, desc) = parse_skill_frontmatter(content);
        assert_eq!(name, "my-skill");
        assert_eq!(desc, "Do the thing quickly");
    }

    #[test]
    fn parse_no_frontmatter() {
        let content = "# Just a heading\nNo frontmatter here.\n";
        let (name, desc) = parse_skill_frontmatter(content);
        assert!(name.is_empty());
        assert!(desc.is_empty());
    }

    #[test]
    fn extract_tokens_filters_short() {
        let tokens = extract_skill_tokens("my-skill", "run the issue pipeline agent");
        let has = |s: &str| tokens.iter().any(|t| t == s);
        // "run", "the" < 4 chars, not short technical terms
        assert!(!has("run"));
        assert!(!has("the"));
        // "issue", "pipeline", "agent" >= 4 chars
        assert!(has("issue"));
        assert!(has("pipeline"));
        assert!(has("agent"));
    }

    #[test]
    fn extract_tokens_includes_short_technical() {
        let tokens = extract_skill_tokens("use-api", "call the api endpoint");
        let has = |s: &str| tokens.iter().any(|t| t == s);
        assert!(has("api"));
    }
}
