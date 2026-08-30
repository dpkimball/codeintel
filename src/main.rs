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
    /// Architectural drift between two indexed Git SHAs (bindb as_of, no source walk).
    Drift {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Federated graph across repositories in one store (no source walk).
    Federation {
        #[arg(long)]
        store: PathBuf,
        /// Crate name or `repo/crate` for cross-repo impact.
        #[arg(long)]
        impact: Option<String>,
    },
    /// Agent-facing architecture API (JSON). Prefer this over graph primitives.
    Architecture {
        #[arg(long)]
        store: PathBuf,
        #[command(subcommand)]
        op: ArchOp,
    },
    /// Autonomous engineering workflow (graph plan → inspect → implement → re-index).
    Workflow {
        #[command(subcommand)]
        op: WorkflowOp,
    },
    /// Index twice (full + incremental no-op) and print validation metrics.
    Validate {
        repo: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Fetch GitHub issues via `gh` CLI and index them for semantic plan() matching.
    IndexIssues {
        /// Full GitHub repo name, e.g. "dpkimball/binder-ai".
        #[arg(long)]
        github_repo: String,
        /// bindb store path.
        #[arg(long)]
        store: PathBuf,
        /// codeintel repo name (defaults to the last segment of github_repo).
        #[arg(long)]
        name: Option<String>,
    },
    /// Walk git log for #NNN commit refs and write REL_RESOLVES edges to the store.
    LinkIssues {
        /// Path to the git repository.
        repo: PathBuf,
        /// bindb store path.
        #[arg(long)]
        store: PathBuf,
        /// codeintel repo name (defaults to the last path segment of repo).
        #[arg(long)]
        name: Option<String>,
    },
    /// Fetch GitHub pull requests via `gh` CLI and index them for semantic plan() matching.
    IndexPrs {
        /// Full GitHub repo name, e.g. "dpkimball/binder-ai".
        #[arg(long)]
        github_repo: String,
        /// bindb store path.
        #[arg(long)]
        store: PathBuf,
        /// codeintel repo name (defaults to the last segment of github_repo).
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum ArchOp {
    Summary,
    Doctrine,
    Impact {
        #[arg(long)]
        target: String,
    },
    Dependencies {
        #[arg(long)]
        target: String,
    },
    Cycles,
    Layering,
    Drift {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    Federation,
    Plan {
        #[arg(long)]
        request: String,
    },
}

#[derive(Subcommand)]
enum WorkflowOp {
    /// Plan from bindb, snapshot doctrine/impact, write a session file.
    Start {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        request: String,
        /// Repeatable `name=/path/to/repo` for later re-index.
        #[arg(long = "bind")]
        binds: Vec<String>,
        #[arg(long)]
        session: PathBuf,
    },
    /// Re-index bound repos, re-check doctrine, emit execution metrics.
    Finish {
        #[arg(long)]
        session: PathBuf,
        /// Paths the agent actually opened (relative to the repo root).
        #[arg(long)]
        opened: Vec<String>,
        /// Paths the agent modified.
        #[arg(long)]
        modified: Vec<String>,
        #[arg(long, value_enum)]
        validation: ValidationStatus,
        #[arg(long, default_value = "")]
        validation_detail: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ValidationStatus {
    Pass,
    Fail,
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
        Cmd::Drift {
            store,
            name,
            from,
            to,
        } => {
            let db = open_store(&store)?;
            let report = codeintel::drift_analyze(&db, &name, &from, &to)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            db.close()?;
        }
        Cmd::Federation { store, impact } => {
            let db = open_store(&store)?;
            if let Some(target) = impact {
                let report = codeintel::federation_impact(&db, &target)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let report = codeintel::federation_analyze(&db)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            db.close()?;
        }
        Cmd::Architecture { store, op } => {
            let arch = codeintel::Architecture::open(&store)?;
            let out = match op {
                ArchOp::Summary => serde_json::to_value(arch.summary()?)?,
                ArchOp::Doctrine => serde_json::to_value(arch.doctrine()?)?,
                ArchOp::Impact { target } => serde_json::to_value(arch.impact(&target)?)?,
                ArchOp::Dependencies { target } => {
                    serde_json::to_value(arch.dependencies(&target)?)?
                }
                ArchOp::Cycles => serde_json::to_value(arch.cycles()?)?,
                ArchOp::Layering => serde_json::to_value(arch.layering()?)?,
                ArchOp::Drift { from, to } => serde_json::to_value(arch.drift(&from, &to)?)?,
                ArchOp::Federation => serde_json::to_value(arch.federation()?)?,
                ArchOp::Plan { request } => serde_json::to_value(arch.plan(&request)?)?,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
            arch.close()?;
        }
        Cmd::Workflow { op } => match op {
            WorkflowOp::Start {
                store,
                request,
                binds,
                session,
            } => {
                let binds = binds
                    .iter()
                    .map(|s| codeintel::workflow::parse_bind(s))
                    .collect::<codeintel::Result<Vec<_>>>()?;
                let out = codeintel::workflow_start(store, &request, binds, session)?;
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
            WorkflowOp::Finish {
                session,
                opened,
                modified,
                validation,
                validation_detail,
            } => {
                let record = codeintel::workflow::ValidationRecord {
                    passed: matches!(validation, ValidationStatus::Pass),
                    detail: validation_detail,
                };
                let out = codeintel::workflow_finish(session, opened, modified, record)?;
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        },
        Cmd::IndexIssues {
            github_repo,
            store,
            name,
        } => {
            let repo_name = name.unwrap_or_else(|| {
                github_repo
                    .rsplit('/')
                    .next()
                    .unwrap_or(&github_repo)
                    .to_string()
            });
            let db = open_store(&store)?;
            let count = codeintel::index_issues(&db, &github_repo, &repo_name)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "github_repo": github_repo,
                    "repo_name": repo_name,
                    "issues_indexed": count,
                }))?
            );
        }
        Cmd::LinkIssues { repo, store, name } => {
            let repo_name = name.unwrap_or_else(|| {
                repo.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repo".into())
            });
            let db = open_store(&store)?;
            let count = codeintel::commit_issues::link_issues_from_commits(&db, &repo_name, &repo)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "repo_name": repo_name,
                    "edges_written": count,
                }))?
            );
        }
        Cmd::IndexPrs {
            github_repo,
            store,
            name,
        } => {
            let repo_name = name.unwrap_or_else(|| {
                github_repo
                    .rsplit('/')
                    .next()
                    .unwrap_or(&github_repo)
                    .to_string()
            });
            let db = open_store(&store)?;
            let count = codeintel::index_prs(&db, &github_repo, &repo_name)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "github_repo": github_repo,
                    "repo_name": repo_name,
                    "prs_indexed": count,
                }))?
            );
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
