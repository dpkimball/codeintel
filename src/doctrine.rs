use crate::error::Result;
use crate::query::{Cycle, QueryEngine};
use serde::Serialize;
use std::collections::BTreeMap;

/// Ranked layers: a crate may depend on the same or a deeper rank,
/// but must not depend on a shallower (more UI-facing) layer.
#[derive(Debug, Clone)]
pub struct LayerDoctrine {
    pub name: String,
    pub crate_rank: BTreeMap<String, u32>,
}

impl LayerDoctrine {
    pub fn detect(crate_names: &[String]) -> Self {
        if crate_names
            .iter()
            .any(|c| c == "binder-ui" || c == "binder-app")
        {
            Self::binder()
        } else {
            Self::flat(crate_names)
        }
    }

    pub fn binder() -> Self {
        let mut crate_rank = BTreeMap::new();
        for c in ["binder-ui", "binder-app", "binder-home", "binder-commands"] {
            crate_rank.insert(c.to_string(), 0);
        }
        for c in [
            "binder-runtime",
            "binder-sdk",
            "binder-platform",
            "binder-services",
            "binder-service-rt",
            "binder-events",
            "binder-workspace",
            "binder-watcher",
            "binder-ingest",
            "binder-telemetry",
            "binder-ai",
            "binder-ai-mistralrs",
            "binder-binders",
            "binder-manifest",
        ] {
            crate_rank.insert(c.to_string(), 1);
        }
        for c in [
            "binder-dpk",
            "binder-core",
            "dpk-core",
            "dpk-query",
            "dpk-graph",
            "dpk-analytics",
            "dpk-semantic",
            "dpk-identity",
        ] {
            crate_rank.insert(c.to_string(), 2);
        }
        crate_rank.insert("binder-storage".into(), 3);
        Self {
            name: "binder".into(),
            crate_rank,
        }
    }

    pub fn flat(crate_names: &[String]) -> Self {
        let mut crate_rank = BTreeMap::new();
        for c in crate_names {
            crate_rank.insert(c.clone(), 0);
        }
        Self {
            name: "flat".into(),
            crate_rank,
        }
    }

    pub fn rank_of(&self, crate_name: &str) -> Option<u32> {
        self.crate_rank.get(crate_name).copied()
    }

    pub fn violates(&self, from_crate: &str, to_crate: &str) -> bool {
        match (self.rank_of(from_crate), self.rank_of(to_crate)) {
            (Some(f), Some(t)) => t < f,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub edge_type: String,
    pub from: String,
    pub to: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule_id: String,
    pub rule: String,
    pub severity: Severity,
    pub evidence: Evidence,
    pub objects: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConformanceReport {
    pub doctrine: String,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
    pub rules_evaluated: usize,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone)]
pub enum Rule {
    Layering {
        id: String,
        layers: LayerDoctrine,
    },
    ForbiddenDep {
        id: String,
        from: String,
        to: String,
        remediation: String,
    },
    Ownership {
        id: String,
        owners: Vec<String>,
        may_depend_on: Vec<String>,
        severity: Severity,
    },
    Acyclic {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub struct Doctrine {
    pub name: String,
    pub rules: Vec<Rule>,
}

impl Doctrine {
    pub fn detect(crate_names: &[String]) -> Self {
        if crate_names
            .iter()
            .any(|c| c == "binder-ui" || c == "binder-app")
        {
            Self::binder()
        } else {
            Self::workspace_default()
        }
    }

    pub fn binder() -> Self {
        let layers = LayerDoctrine::binder();
        Self {
            name: "binder".into(),
            rules: vec![
                Rule::Layering {
                    id: "binder.layering".into(),
                    layers,
                },
                Rule::Acyclic {
                    id: "binder.acyclic-crates".into(),
                },
                Rule::ForbiddenDep {
                    id: "binder.storage-not-ingest".into(),
                    from: "binder-storage".into(),
                    to: "binder-ingest".into(),
                    remediation: "Keep ingest above storage; storage should not depend on the ingest pipeline. Invert or extract a shared contract crate.".into(),
                },
                Rule::ForbiddenDep {
                    id: "binder.storage-not-sdk".into(),
                    from: "binder-storage".into(),
                    to: "binder-sdk".into(),
                    remediation: "Storage depending on binder-sdk inverts ownership. Depend on a narrower types crate or push the SDK call up.".into(),
                },
                Rule::Ownership {
                    id: "binder.domain-isolation".into(),
                    owners: vec![
                        "dpk-*".into(),
                        "binder-dpk".into(),
                        "binder-core".into(),
                    ],
                    may_depend_on: vec![
                        "dpk-*".into(),
                        "binder-dpk".into(),
                        "binder-core".into(),
                        "binder-sdk".into(),
                        "binder-events".into(),
                        "binder-storage".into(),
                    ],
                    severity: Severity::Warning,
                },
            ],
        }
    }

    pub fn workspace_default() -> Self {
        Self {
            name: "workspace".into(),
            rules: vec![Rule::Acyclic {
                id: "workspace.acyclic-crates".into(),
            }],
        }
    }

    /// Evaluate crate `DEPENDS_ON` edges in bindb. Does not open source files.
    pub fn evaluate(&self, engine: &QueryEngine) -> Result<ConformanceReport> {
        self.evaluate_graph(&engine.name, &engine.crate_dep_graph()?)
    }

    /// Same rules as `evaluate`, against an already reconstructed dep graph.
    pub fn evaluate_graph(
        &self,
        repo: &str,
        edges: &[(String, String)],
    ) -> Result<ConformanceReport> {
        let mut findings = Vec::new();
        for rule in &self.rules {
            match rule {
                Rule::Layering { id, layers } => {
                    for (from, to) in edges {
                        if layers.violates(from, to) {
                            let from_rank = layers.rank_of(from).unwrap_or(0);
                            let to_rank = layers.rank_of(to).unwrap_or(0);
                            findings.push(Finding {
                                rule_id: id.clone(),
                                rule: format!(
                                    "Dependency direction: layer {from_rank} must not depend on shallower layer {to_rank}"
                                ),
                                severity: Severity::Error,
                                evidence: Evidence {
                                    edge_type: crate::ids::REL_DEPENDS_ON.into(),
                                    from: from.clone(),
                                    to: to.clone(),
                                    detail: format!("{from} (rank {from_rank}) DEPENDS_ON {to} (rank {to_rank})"),
                                },
                                objects: vec![
                                    crate::ids::crate_id(repo, from).0,
                                    crate::ids::crate_id(repo, to).0,
                                ],
                                remediation: format!(
                                    "Remove {from} → {to}, or move the needed types into rank ≥ {from_rank}."
                                ),
                            });
                        }
                    }
                }
                Rule::ForbiddenDep {
                    id,
                    from,
                    to,
                    remediation,
                } => {
                    for (a, b) in edges {
                        if glob_match(from, a) && glob_match(to, b) {
                            findings.push(Finding {
                                rule_id: id.clone(),
                                rule: format!("Forbidden dependency: {from} ↛ {to}"),
                                severity: Severity::Error,
                                evidence: Evidence {
                                    edge_type: crate::ids::REL_DEPENDS_ON.into(),
                                    from: a.clone(),
                                    to: b.clone(),
                                    detail: format!("direct DEPENDS_ON {a} → {b}"),
                                },
                                objects: vec![
                                    crate::ids::crate_id(repo, a).0,
                                    crate::ids::crate_id(repo, b).0,
                                ],
                                remediation: remediation.clone(),
                            });
                        }
                    }
                }
                Rule::Ownership {
                    id,
                    owners,
                    may_depend_on,
                    severity,
                } => {
                    for (a, b) in edges {
                        if !owners.iter().any(|p| glob_match(p, a)) {
                            continue;
                        }
                        if owners.iter().any(|p| glob_match(p, b)) {
                            continue;
                        }
                        if may_depend_on.iter().any(|p| glob_match(p, b)) {
                            continue;
                        }
                        findings.push(Finding {
                            rule_id: id.clone(),
                            rule: "Ownership boundary: owner crate depends on a crate outside its allow-list".into(),
                            severity: *severity,
                            evidence: Evidence {
                                edge_type: crate::ids::REL_DEPENDS_ON.into(),
                                from: a.clone(),
                                to: b.clone(),
                                detail: format!("{a} is an owner crate but DEPENDS_ON {b}"),
                            },
                            objects: vec![
                                crate::ids::crate_id(repo, a).0,
                                crate::ids::crate_id(repo, b).0,
                            ],
                            remediation: format!(
                                "Keep {a}'s dependencies inside the ownership set, or extend the allow-list deliberately."
                            ),
                        });
                    }
                }
                Rule::Acyclic { id } => {
                    let cycles = crate::query::crate_cycles(edges);
                    for Cycle { nodes } in cycles {
                        let objects: Vec<String> = nodes
                            .iter()
                            .map(|n| crate::ids::crate_id(repo, n).0)
                            .collect();
                        let path = nodes.join(" → ");
                        findings.push(Finding {
                            rule_id: id.clone(),
                            rule: "Crate dependency graph must be acyclic".into(),
                            severity: Severity::Error,
                            evidence: Evidence {
                                edge_type: crate::ids::REL_DEPENDS_ON.into(),
                                from: nodes.first().cloned().unwrap_or_default(),
                                to: nodes.last().cloned().unwrap_or_default(),
                                detail: format!("cycle {path}"),
                            },
                            objects,
                            remediation: "Break the cycle by extracting a shared contract crate or inverting one edge.".into(),
                        });
                    }
                }
            }
        }
        Ok(ConformanceReport {
            doctrine: self.name.clone(),
            primary_source: "bindb",
            source_files_opened: 0,
            rules_evaluated: self.rules.len(),
            findings,
        })
    }
}

pub fn glob_match(pattern: &str, name: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        pattern == name
    }
}
