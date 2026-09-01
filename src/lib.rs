//! Code intelligence domain: Git-synced semantic graph stored in bindb.

/// Agents plan from bindb and open source only to implement or verify.
pub const WORKFLOW_PROTOCOL: &str = "bindb-first";

pub mod architecture;
pub mod cargo_graph;
pub mod commit_issues;
pub mod config_keys;
pub mod doctrine;
pub mod drift;
pub mod error;
pub mod federation;
pub mod git_sync;
pub mod github_issues;
pub mod ids;
pub mod impact;
pub mod indexer;
pub mod model;
pub mod parser;
pub mod python_parser;
pub mod query;
pub mod question_bank;
pub mod report;
pub mod rust_parser;
pub mod ts_parser;
pub mod workflow;

pub use architecture::Architecture;
pub use doctrine::{ConformanceReport, Doctrine};
pub use drift::{analyze as drift_analyze, DriftReport};
pub use error::{CodeintelError, Result};
pub use federation::{
    analyze as federation_analyze, impact as federation_impact, FederationImpact, FederationReport,
};
pub use github_issues::{index_issues, index_prs};
pub use impact::{analyze as impact_analyze, ImpactReport};
pub use indexer::{index, IndexOptions, IndexStats};
pub use query::QueryEngine;
pub use question_bank::{
    content_hash, delete_question, export_questions, get_question, index_questions,
    index_questions_with_opts, patch_question_answer, patch_question_with_opts, put_question,
    put_question_with_opts, reactivate_question, retire_question, QuestionWriteOpts,
};
pub use report::{run_validation, ValidationReport};
pub use workflow::{finish as workflow_finish, start as workflow_start, WorkflowReport};
