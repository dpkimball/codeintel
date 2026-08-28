//! Code intelligence domain: Git-synced semantic graph stored in bindb.

pub mod cargo_graph;
pub mod doctrine;
pub mod error;
pub mod git_sync;
pub mod ids;
pub mod impact;
pub mod indexer;
pub mod model;
pub mod parser;
pub mod query;
pub mod report;
pub mod rust_parser;

pub use doctrine::{ConformanceReport, Doctrine};
pub use error::{CodeintelError, Result};
pub use impact::{analyze as impact_analyze, ImpactReport};
pub use indexer::{index, IndexOptions, IndexStats};
pub use query::QueryEngine;
pub use report::{run_validation, ValidationReport};
