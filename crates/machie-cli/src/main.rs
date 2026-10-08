//! `machie` — the Machie CLI entrypoint.
//!
//! v0.1.0 surface: version / status / init / config-check / db-path / session commands.
//! Design rule: bad input produces a friendly one-line error and exit code 1, never a panic.

mod sessions;

use clap::{Parser, Subcommand};
use machie_config::Config;
use machie_db::Db;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "machie",
    version = version_string(),
    about = "Machie — your machine's second brain (early days)",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show app and environment status
    Status,
    /// Write a starter config file to the user config directory
    Init,
    /// Validate all discovered config layers
    ConfigCheck,
    /// Show where data lives
    DbPath,
    /// Manage chat/task sessions (stored in SQLite)
    Session {
        #[command(subcommand)]
        action: sessions::SessionCmd,
    },
}

fn version_string() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), " (v0.1 foundation)")
}

/// Resolve the database location from effective config.
fn db_path(cfg: &Config) -> PathBuf {
    cfg.data_dir().join("machie.db")
}

fn open_db_at(path: std::path::PathBuf) -> Result<Db, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(Db::open(&path)?)
}

/// Uniform error presentation: friendly message on stderr, exit code 1.
fn fail(err: impl std::fmt::Display) -> ! {
    eprintln!("machie: {err}");
    eprintln!("(no changes were made)");
    std::process::exit(1)
}

fn main() {
    let cli = Cli::parse();

    // Config discovery itself can fail (bad TOML, future version) — report it kindly.
    let cfg = match Config::discover() {
        Ok(c) => c,
        Err(e) => fail(e),
    };

    match cli.command {
        None | Some(Cmd::Status) => {
            println!("Machie {}", version_string());
            println!("  config:   {}", describe_config_layers());
            println!("  data dir: {}", cfg.data_dir().display());
            let dbfile = db_path(&cfg);
            match open_db_at(dbfile) {
                Ok(db) => println!(
                    "  database: {} (schema v{}, {} session(s))",
                    db.path(),
                    db.schema_version(),
                    db.list_sessions().map(|v| v.len()).unwrap_or(0)
                ),
                Err(e) => fail(e),
            }
            println!("  next:     sessions are ready for real conversations in v0.2+");
        }
        Some(Cmd::Init) => {
            let path = Config::default_user_config_path();
            if path.exists() {
                fail(format!(
                    "config already exists at {}; edit it directly or delete it first",
                    path.display()
                ));
            }
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    fail(e);
                }
            }
            if let Err(e) = std::fs::write(&path, Config::STARTER_TOML) {
                fail(e);
            }
            println!("Wrote starter config to {}", path.display());
        }
        Some(Cmd::ConfigCheck) => {
            println!("Config OK (version {}).", cfg.config_version.unwrap_or(1));
            println!("  merged settings:");
            println!("    paths.data_dir = {:?}", cfg.paths.data_dir);
            println!("    log.level      = {:?}", cfg.log.level);
        }
        Some(Cmd::DbPath) => println!("{}", db_path(&cfg).display()),
        Some(Cmd::Session { action }) => {
            let db = match open_db_at(db_path(&cfg)) {
                Ok(db) => db,
                Err(e) => fail(e),
            };
            if let Err(e) = sessions::run(action, &db) {
                fail(e);
            }
        }
    }
}

fn describe_config_layers() -> String {
    let mut parts = Vec::new();
    if let Ok(p) = std::env::var("MACHIE_CONFIG") {
        parts.push(format!("$MACHIE_CONFIG={p}"));
    }
    parts.push(format!(
        "user={}",
        Config::default_user_config_path().display()
    ));
    if PathBuf::from("machie.toml").is_file() {
        parts.push("project=./machie.toml".to_string());
    }
    parts.join(", ")
}
