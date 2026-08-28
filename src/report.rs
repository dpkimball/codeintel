use crate::error::Result;
use crate::indexer::IndexStats;
use crate::query::{QueryEngine, QueryTimed};
use bindb::BinderStore;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub index: IndexStats,
    pub incremental: IndexStats,
    pub token_reduction: TokenReduction,
    pub queries: QueryLatencies,
    pub summary: crate::query::ArchitectureSummary,
    pub cycles: QueryTimed<Vec<crate::query::Cycle>>,
    pub layering: QueryTimed<Vec<crate::query::LayerViolation>>,
    pub unused_public: QueryTimed<usize>,
    pub duplicates: QueryTimed<usize>,
    pub mermaid: String,
}

#[derive(Debug, Serialize)]
pub struct TokenReduction {
    pub source_bytes: u64,
    pub model_bytes: u64,
    /// Rough GPT-style token estimate: bytes / 4.
    pub source_tokens_est: u64,
    pub model_tokens_est: u64,
    pub reduction_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct QueryLatencies {
    pub summary_ms: u64,
    pub cycles_ms: u64,
    pub layering_ms: u64,
    pub unused_ms: u64,
    pub duplicates_ms: u64,
    pub fan_ms: u64,
    pub hotspots_ms: u64,
}

pub fn run_validation(
    store: &BinderStore,
    name: &str,
    first: IndexStats,
    second: IndexStats,
) -> Result<ValidationReport> {
    let q = QueryEngine::open(store, name);
    let summary = q.timed(|| q.summary())?;
    let cycles = q.timed(|| q.circular_dependencies())?;
    let layering = q.timed(|| q.layering_violations())?;
    let unused = q.timed(|| q.unused_public_apis().map(|v| v.len()))?;
    let duplicates = q.timed(|| q.duplicates().map(|v| v.len()))?;
    let fan = q.timed(|| q.fan_stats(10))?;
    let hotspots = q.timed(|| q.hotspots(10))?;
    let mermaid = q.mermaid_crate_graph()?;
    let source_bytes = first.source_bytes;
    let model_bytes = first.model_bytes;
    let source_tokens_est = source_bytes / 4;
    let model_tokens_est = model_bytes / 4;
    let reduction_ratio = if source_tokens_est == 0 {
        0.0
    } else {
        1.0 - (model_tokens_est as f64 / source_tokens_est as f64)
    };
    Ok(ValidationReport {
        index: first,
        incremental: second,
        token_reduction: TokenReduction {
            source_bytes,
            model_bytes,
            source_tokens_est,
            model_tokens_est,
            reduction_ratio,
        },
        queries: QueryLatencies {
            summary_ms: summary.latency_ms,
            cycles_ms: cycles.latency_ms,
            layering_ms: layering.latency_ms,
            unused_ms: unused.latency_ms,
            duplicates_ms: duplicates.latency_ms,
            fan_ms: fan.latency_ms,
            hotspots_ms: hotspots.latency_ms,
        },
        summary: summary.result,
        cycles,
        layering,
        unused_public: unused,
        duplicates,
        mermaid,
    })
}
