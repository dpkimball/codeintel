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
    /// Walk a skills directory (.cursor/skills/ or .claude/skills/) and index SKILL.md files into bindb for plan() skill discovery.
    IndexSkills {
        /// Path to the skills root directory (e.g. /path/to/.cursor/skills).
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        store: PathBuf,
        /// Namespace for skill IDs, e.g. "dpk-workspace".
        #[arg(long, default_value = "dpk-workspace")]
        name: String,
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
    /// Parse question-bank.md and index Q&A pairs into the store for plan() guidance.
    IndexQuestions {
        /// bindb store path.
        #[arg(long)]
        store: PathBuf,
        /// Path to question-bank.md.
        #[arg(long)]
        bank: PathBuf,
        /// Replace active rows even when import_base_version mismatches (never reactivates retired/deleted).
        #[arg(long)]
        force: bool,
    },
    /// Export indexed questions to markdown and refresh import provenance.
    ExportQuestions {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        bank: PathBuf,
    },
    /// Upsert one Q&A pair into bindb (API / manual authoring without re-indexing the bank file).
    PutQuestion {
        /// bindb store path.
        #[arg(long)]
        store: PathBuf,
        /// codeintel repo name, e.g. "codeintel", or "" / "cross" for cross-repo.
        #[arg(long)]
        repo: String,
        /// Question id within the repo section, e.g. "C7".
        #[arg(long)]
        id: String,
        #[arg(long)]
        question: String,
        #[arg(long)]
        answer: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        expected_version: Option<u64>,
    },
    /// Update answer and/or append aliases on an existing question.
    PatchQuestion {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        answer: Option<String>,
        /// Comma-separated alias strings to append.
        #[arg(long, value_delimiter = ',')]
        aliases: Vec<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        expected_version: Option<u64>,
    },
    RetireQuestion {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        expected_version: Option<u64>,
    },
    ReactivateQuestion {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        expected_version: Option<u64>,
    },
    DeleteQuestion {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        expected_version: Option<u64>,
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
        /// Compare against this indexed SHA to populate recent_changes.
        #[arg(long)]
        since_sha: Option<String>,
    },
    /// All indexed markdown docs with structural compliance violations.
    DocCompliance,
    /// Question-bank token overlap only (no graph plan).
    Guidance {
        #[arg(long)]
        request: String,
        /// Limit bank scan to this repo plus cross-repo rows (fast `/ask` path).
        #[arg(long)]
        repo: Option<String>,
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
                ArchOp::Plan { request, since_sha } => {
                    let report = match since_sha.as_deref() {
                        Some(sha) => arch.plan_since(&request, sha)?,
                        None => arch.plan(&request)?,
                    };
                    serde_json::to_value(report)?
                }
                ArchOp::DocCompliance => serde_json::to_value(arch.doc_compliance()?)?,
                ArchOp::Guidance { request, repo } => {
                    serde_json::to_value(arch.guidance(&request, repo.as_deref())?)?
                }
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
        Cmd::IndexSkills { dir, store, name } => {
            let db = open_store(&store)?;
            let count = codeintel::index_skills(&db, &dir, &name)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "namespace": name,
                    "dir": dir.display().to_string(),
                    "skills_indexed": count,
                }))?
            );
        }
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
        Cmd::IndexQuestions { store, bank, force } => {
            let db = open_store(&store)?;
            let report = codeintel::index_questions_with_opts(&db, &bank, force)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "bank": bank.display().to_string(),
                    "questions_indexed": report.indexed,
                    "provenance_synced": report.synced,
                    "conflicts": report.conflicts,
                    "rows_parsed": report.rows_parsed,
                    "rows_active": report.rows_active,
                }))?
            );
        }
        Cmd::ExportQuestions { store, bank } => {
            let db = open_store(&store)?;
            let count = codeintel::export_questions(&db, &bank)?;
            db.close()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "bank": bank.display().to_string(),
                    "questions_exported": count,
                }))?
            );
        }
        Cmd::PutQuestion {
            store,
            repo,
            id,
            question,
            answer,
            reason,
            expected_version,
        } => run_question_write(|| {
            let repo_name = normalize_repo(&repo);
            let db = open_store(&store)?;
            let rec = codeintel::put_question_with_opts(
                &db,
                &repo_name,
                &id,
                &question,
                &answer,
                question_write_opts(reason.as_deref(), expected_version),
            )?;
            db.close()?;
            print_question_record(&repo_name, &id, rec, "stored")
        })?,
        Cmd::PatchQuestion {
            store,
            repo,
            id,
            answer,
            aliases,
            reason,
            expected_version,
        } => run_question_write(|| {
            let repo_name = normalize_repo(&repo);
            let db = open_store(&store)?;
            let answer_ref = answer.as_deref();
            let aliases_ref = if aliases.is_empty() {
                None
            } else {
                Some(aliases.as_slice())
            };
            let rec = codeintel::patch_question_with_opts(
                &db,
                &repo_name,
                &id,
                answer_ref,
                aliases_ref,
                question_write_opts(reason.as_deref(), expected_version),
            )?;
            db.close()?;
            print_question_record(&repo_name, &id, rec, "updated")
        })?,
        Cmd::RetireQuestion {
            store,
            repo,
            id,
            reason,
            expected_version,
        } => run_question_write(|| {
            let repo_name = normalize_repo(&repo);
            let db = open_store(&store)?;
            let rec = codeintel::retire_question(
                &db,
                &repo_name,
                &id,
                question_write_opts(reason.as_deref(), expected_version),
            )?;
            db.close()?;
            print_question_record(&repo_name, &id, rec, "retired")
        })?,
        Cmd::ReactivateQuestion {
            store,
            repo,
            id,
            reason,
            expected_version,
        } => run_question_write(|| {
            let repo_name = normalize_repo(&repo);
            let db = open_store(&store)?;
            let rec = codeintel::reactivate_question(
                &db,
                &repo_name,
                &id,
                question_write_opts(reason.as_deref(), expected_version),
            )?;
            db.close()?;
            print_question_record(&repo_name, &id, rec, "reactivated")
        })?,
        Cmd::DeleteQuestion {
            store,
            repo,
            id,
            reason,
            expected_version,
        } => run_question_write(|| {
            let repo_name = normalize_repo(&repo);
            let db = open_store(&store)?;
            let rec = codeintel::delete_question(
                &db,
                &repo_name,
                &id,
                question_write_opts(reason.as_deref(), expected_version),
            )?;
            db.close()?;
            print_question_record(&repo_name, &id, rec, "deleted")
        })?,
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

fn normalize_repo(repo: &str) -> String {
    if repo == "cross" {
        String::new()
    } else {
        repo.to_string()
    }
}

fn question_write_opts<'a>(
    reason: Option<&'a str>,
    expected_version: Option<u64>,
) -> codeintel::QuestionWriteOpts<'a> {
    codeintel::QuestionWriteOpts {
        reason,
        expected_version,
    }
}

fn run_question_write(
    op: impl FnOnce() -> codeintel::Result<serde_json::Value>,
) -> codeintel::Result<()> {
    match op() {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v)?);
            Ok(())
        }
        Err(codeintel::CodeintelError::StaleVersion {
            repo,
            id,
            current_version,
        }) => {
            let body = serde_json::json!({
                "error": "stale_version",
                "repo": repo,
                "id": id,
                "current_version": current_version,
            });
            eprintln!("{}", serde_json::to_string_pretty(&body)?);
            std::process::exit(2);
        }
        Err(err) => Err(err),
    }
}

fn print_question_record(
    repo: &str,
    id: &str,
    rec: codeintel::model::QuestionRecord,
    action: &str,
) -> codeintel::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "repo": repo,
        "id": id,
        action: true,
        "version": rec.version,
        "status": rec.status,
    }))
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
