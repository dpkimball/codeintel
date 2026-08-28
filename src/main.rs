use clap::{Parser, Subcommand, ValueEnum};
use codeintel::indexer::{index, open_store, IndexOptions};
use codeintel::query::QueryEngine;
use codeintel::report::run_validation;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "codeintel",
    about = "Git-synced semantic graph in bindb for autonomous agents"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Incrementally index a Git repository into a bindb store.
    Index {
        repo: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Run an architectural query against an indexed store.
    Query {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long, value_enum)]
        question: Question,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
    /// Evaluate architectural doctrine against the bindb graph (no source walk).
    Doctrine {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "auto")]
        profile: String,
    },
    /// Change impact for a crate, module, file, or symbol — bindb only.
    Impact {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        target: String,
    },
    /// Index twice (full + incremental no-op) and print validation metrics.
    Validate {
        repo: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Question {
    Summary,
    DepGraph,
    Cycles,
    Layering,
    Unused,
    Duplicates,
    Fan,
    Impact,
    Callers,
    Impls,
    Paths,
    Hotspots,
    Patterns,
    Antipatterns,
    Diagram,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("codeintel: {e}");
        std::process::exit(1);
    }
}

fn run() -> codeintel::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Index { repo, store, name } => {
            let stats = index(IndexOptions {
                repo_root: repo,
                store_path: store,
                name,
            })?;
            println!("{}", serde_json::to_string_pretty(&json_stats(&stats))?);
        }
        Cmd::Query {
            store,
            name,
            question,
            symbol,
            from,
            to,
        } => {
            let db = open_store(&store)?;
            let q = QueryEngine::open(&db, name);
            let out = match question {
                Question::Summary => serde_json::to_value(q.summary()?)?,
                Question::DepGraph => serde_json::to_value(q.crate_dep_graph()?)?,
                Question::Cycles => serde_json::to_value(q.circular_dependencies()?)?,
                Question::Layering => serde_json::to_value(q.layering_violations()?)?,
                Question::Unused => serde_json::to_value(q.unused_public_apis()?)?,
                Question::Duplicates => serde_json::to_value(q.duplicates()?)?,
                Question::Fan => serde_json::to_value(q.fan_stats(25)?)?,
                Question::Impact => {
                    serde_json::to_value(q.impact(symbol.as_deref().unwrap_or(""))?)?
                }
                Question::Callers => {
                    serde_json::to_value(q.callers(symbol.as_deref().unwrap_or(""))?)?
                }
                Question::Impls => {
                    serde_json::to_value(q.trait_impls(symbol.as_deref().unwrap_or(""))?)?
                }
                Question::Paths => serde_json::to_value(
                    q.paths(from.as_deref().unwrap_or(""), to.as_deref().unwrap_or(""))?,
                )?,
                Question::Hotspots => serde_json::to_value(q.hotspots(20)?)?,
                Question::Patterns => serde_json::to_value(q.patterns()?)?,
                Question::Antipatterns => serde_json::to_value(q.antipatterns()?)?,
                Question::Diagram => serde_json::to_value(q.mermaid_crate_graph()?)?,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
            db.close()?;
        }
        Cmd::Doctrine {
            store,
            name,
            profile,
        } => {
            let db = open_store(&store)?;
            let q = QueryEngine::open(&db, name);
            let crates: Vec<String> = q.crates()?.into_iter().map(|c| c.name).collect();
            let doctrine = match profile.as_str() {
                "binder" => codeintel::Doctrine::binder(),
                "workspace" => codeintel::Doctrine::workspace_default(),
                _ => codeintel::Doctrine::detect(&crates),
            };
            let report = doctrine.evaluate(&q)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            db.close()?;
        }
        Cmd::Impact {
            store,
            name,
            target,
        } => {
            let db = open_store(&store)?;
            let q = QueryEngine::open(&db, name);
            let report = codeintel::impact_analyze(&q, &target)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            db.close()?;
        }
        Cmd::Validate { repo, store, name } => {
            let name = name.or_else(|| repo.file_name().map(|s| s.to_string_lossy().into_owned()));
            let opts = IndexOptions {
                repo_root: repo.clone(),
                store_path: store.clone(),
                name: name.clone(),
            };
            let first = index(IndexOptions {
                repo_root: repo.clone(),
                store_path: store.clone(),
                name: name.clone(),
            })?;
            let second = index(opts)?;
            let db = open_store(&store)?;
            let resolved = name.unwrap_or_else(|| "repo".into());
            let report = run_validation(&db, &resolved, first, second)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            db.close()?;
        }
    }
    Ok(())
}

fn json_stats(s: &codeintel::IndexStats) -> serde_json::Value {
    serde_json::json!({
        "name": s.name,
        "sha": s.sha,
        "branch": s.branch,
        "skipped": s.skipped,
        "files_seen": s.files_seen,
        "files_indexed": s.files_indexed,
        "skipped_unchanged": s.skipped_unchanged,
        "symbol_count": s.symbol_count,
        "source_bytes": s.source_bytes,
        "model_bytes": s.model_bytes,
        "duration_ms": s.duration_ms,
        "store_bytes": s.store_bytes,
    })
}
