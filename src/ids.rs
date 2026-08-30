use bindb::{EdgeId, ObjectId};
use sha2::{Digest, Sha256};

pub const SCHEME: &str = "code://";
pub const ACTOR: &str = "codeintel";
pub const NAMESPACE: &str = "code";

pub const REL_CONTAINS: &str = "CONTAINS";
pub const REL_DEFINED_IN: &str = "DEFINED_IN";
pub const REL_IMPORTS: &str = "IMPORTS";
pub const REL_CALLS: &str = "CALLS";
pub const REL_REFERENCES: &str = "REFERENCES";
pub const REL_IMPLEMENTS: &str = "IMPLEMENTS";
pub const REL_DEPENDS_ON: &str = "DEPENDS_ON";
pub const REL_AT_COMMIT: &str = "AT_COMMIT";
pub const REL_IN_REPO: &str = "IN_REPO";
pub const REL_PARENT_OF: &str = "PARENT_OF";

pub fn repo_id(name: &str) -> ObjectId {
    ObjectId::new(format!("code://repo/{name}"))
}

pub fn commit_id(name: &str, sha: &str) -> ObjectId {
    ObjectId::new(format!("code://commit/{name}/{sha}"))
}

pub fn file_id(name: &str, path: &str) -> ObjectId {
    ObjectId::new(format!("code://file/{name}/{path}"))
}

pub fn module_id(name: &str, module: &str) -> ObjectId {
    ObjectId::new(format!("code://mod/{name}/{module}"))
}

pub fn crate_id(name: &str, crate_name: &str) -> ObjectId {
    ObjectId::new(format!("code://crate/{name}/{crate_name}"))
}

pub fn symbol_id(name: &str, qual: &str) -> ObjectId {
    ObjectId::new(format!("code://sym/{name}/{qual}"))
}

pub fn config_id(name: &str, path: &str, key: &str) -> ObjectId {
    ObjectId::new(format!("code://config/{name}/{path}/{key}"))
}

pub fn sync_id(name: &str) -> ObjectId {
    ObjectId::new(format!("code://sync/{name}"))
}

pub fn edge_id(rel: &str, from: &ObjectId, to: &ObjectId) -> EdgeId {
    let mut hasher = Sha256::new();
    hasher.update(rel.as_bytes());
    hasher.update([0u8]);
    hasher.update(from.0.as_bytes());
    hasher.update([0u8]);
    hasher.update(to.0.as_bytes());
    let digest = hasher.finalize();
    EdgeId::new(format!("code://e/{}", to_hex(&digest[..16])))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    to_hex(&digest)
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn is_code_id(id: &ObjectId) -> bool {
    id.0.starts_with(SCHEME)
}

pub fn prefix_file(name: &str) -> String {
    format!("code://file/{name}/")
}

pub fn prefix_sym(name: &str) -> String {
    format!("code://sym/{name}/")
}

pub fn prefix_mod(name: &str) -> String {
    format!("code://mod/{name}/")
}

pub fn prefix_crate(name: &str) -> String {
    format!("code://crate/{name}/")
}

pub fn prefix_config(name: &str) -> String {
    format!("code://config/{name}/")
}

pub fn prefix_commit(name: &str) -> String {
    format!("code://commit/{name}/")
}

pub fn issue_id(repo_name: &str, number: u64) -> ObjectId {
    ObjectId::new(format!("code://issue/{repo_name}/{number}"))
}

pub fn doc_id(repo_name: &str, path: &str) -> ObjectId {
    ObjectId::new(format!("code://doc/{repo_name}/{path}"))
}

pub fn prefix_issue(name: &str) -> String {
    format!("code://issue/{name}/")
}

pub fn prefix_doc(name: &str) -> String {
    format!("code://doc/{name}/")
}

pub const PREFIX_ALL_CRATES: &str = "code://crate/";
pub const PREFIX_ALL_REPOS: &str = "code://repo/";
pub const PREFIX_ALL_COMMITS: &str = "code://commit/";

/// Short technical terms that are meaningful despite being < 4 chars.
/// Indexed and matched regardless of the standard length filter.
pub const SHORT_TECHNICAL_TERMS: &[&str] = &[
    "api", "cli", "sdk", "llm", "rag", "gpu", "cpu", "pr", "ci", "cd", "gh", "url", "uri", "git",
    "rpc", "grpc", "http", "json", "toml", "yaml", "wasm", "fn",
];

/// `code://crate/{repo}/{crate}` → `(repo, crate)`.
pub fn parse_crate_id(id: &ObjectId) -> Option<(String, String)> {
    let rest = id.0.strip_prefix(PREFIX_ALL_CRATES)?;
    let (repo, crate_name) = rest.split_once('/')?;
    if repo.is_empty() || crate_name.is_empty() {
        return None;
    }
    Some((repo.to_string(), crate_name.to_string()))
}

/// `code://commit/{repo}/{sha}` → `(repo, sha)`.
pub fn parse_commit_id(id: &ObjectId) -> Option<(String, String)> {
    let rest = id.0.strip_prefix(PREFIX_ALL_COMMITS)?;
    let (repo, sha) = rest.split_once('/')?;
    if repo.is_empty() || sha.is_empty() {
        return None;
    }
    Some((repo.to_string(), sha.to_string()))
}
