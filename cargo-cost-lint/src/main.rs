#![allow(dead_code)]
mod budget_config;
pub mod cache;
mod config;
mod error;
#[allow(dead_code)]
mod lint_name_set;
mod output_formatters;

use clap::{ArgGroup, Parser, ValueEnum};
use output_formatters::{LintFinding, OutputFormat, Span};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, exit};

#[derive(Parser, Debug)]
#[command(name = "cargo-cost-lint")]
#[command(version = long_version())]
#[command(about = "CLI wrapper for soroban-cost-linter")]
#[command(group(
    ArgGroup::new("verbosity")
        .args(["quiet", "verbose"])
        .multiple(false)
))]
struct Cli {
    #[arg(long, help = "Path to budget.toml")]
    config: Option<String>,

    #[arg(long, help = "Emit the lint inventory and exit")]
    list_lints: bool,

    #[arg(
        long,
        value_name = "LINT",
        help = "Print the full documentation page for a lint and exit"
    )]
    explain: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text, help = "Output format")]
    format: OutputFormat,

    #[arg(long, help = "Suppress informational and warning output")]
    quiet: bool,

    #[arg(
        long,
        help = "Show diagnostic detail: config path, lint flags, spawned command"
    )]
    verbose: bool,

    #[arg(
        long = "allow",
        short = 'A',
        value_name = "LINT",
        action = clap::ArgAction::Append,
        help = "Allow a lint for this run (overrides budget.toml)"
    )]
    allow: Vec<String>,

    #[arg(
        long = "warn",
        short = 'W',
        value_name = "LINT",
        action = clap::ArgAction::Append,
        help = "Set a lint to warning for this run (overrides budget.toml)"
    )]
    warn: Vec<String>,

    #[arg(
        long = "deny",
        short = 'D',
        value_name = "LINT",
        action = clap::ArgAction::Append,
        help = "Deny a lint for this run (overrides budget.toml)"
    )]
    deny: Vec<String>,

    #[arg(
        long = "package",
        short = 'p',
        value_name = "SPEC",
        action = clap::ArgAction::Append,
        help = "Package(s) to lint (repeatable)"
    )]
    package: Vec<String>,

    #[arg(long = "workspace", help = "Lint all packages in the workspace")]
    workspace: bool,

    #[arg(long = "no-cache", help = "Bypass the lint result cache for this run")]
    no_cache: bool,

    #[arg(long = "clear-cache", help = "Clear the lint result cache and exit")]
    clear_cache: bool,

    /// Control coloured output: auto, always, never.
    ///
    /// When set to *auto* (the default), colour is enabled only when
    /// standard output is a terminal.  Honours the widely-adopted
    /// `NO_COLOR` convention (https://no-color.org/): if this flag is
    /// omitted and the `NO_COLOR` environment variable is set to any
    /// non-empty value, output is uncoloured.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, value_name = "WHEN")]
    color: ColorChoice,

    #[arg(long = "no-progress", help = "Disable the progress indicator while running linter")]
    no_progress: bool,
}

#[derive(Deserialize, Debug)]
pub struct BudgetConfig {
    pub lints: Option<std::collections::HashMap<String, String>>,
}

/// Colour-policy preference forwarded to the underlying `cargo dylint`
/// (and therefore `rustc`) invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ColorChoice {
    /// Emit ANSI colours only when stdout is a terminal (default behaviour).
    Auto,
    /// Always emit ANSI colours, even when piped or redirected.
    Always,
    /// Never emit ANSI colours.
    Never,
}

impl ColorChoice {
    /// Return the `cargo dylint` / `cargo check` `--color` argument
    /// value, or `None` when we should let cargo pick its default
    /// (i.e. when colour is desired and there is nothing to override).
    fn as_cargo_arg(&self) -> Option<&'static str> {
        match self {
            ColorChoice::Auto => None,
            ColorChoice::Always => Some("always"),
            ColorChoice::Never => Some("never"),
        }
    }
}

/// Determine the effective colour preference by merging:
///
/// 1. An explicit `--color` CLI flag (highest priority).
/// 2. The `NO_COLOR` environment variable (set to any non-empty value
///    means "no colour").
/// 3. Cargo's built-in default (`Auto`) — colour when stdout is a
///    terminal, no colour otherwise.
fn resolve_color_choice(cli_color: &ColorChoice) -> ColorChoice {
    match cli_color {
        ColorChoice::Auto => {
            // Honour the NO_COLOR convention (https://no-color.org/).
            if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
                ColorChoice::Never
            } else {
                ColorChoice::Auto
            }
        }
        other => *other,
    }
}

include!(concat!(env!("OUT_DIR"), "/lint_names.rs"));
include!(concat!(env!("OUT_DIR"), "/lint_metadata.rs"));
include!(concat!(env!("OUT_DIR"), "/lint_info.rs"));
include!(concat!(env!("OUT_DIR"), "/lint_explanations.rs"));
include!(concat!(env!("OUT_DIR"), "/version_info.rs"));

/// Build the `--version` string.
///
/// First line is the conventional `name version` shape.
/// Subsequent lines report the pinned nightly toolchain and the
/// expected cargo-dylint version constraint — the two pieces of
/// information most often needed when triaging build or lint issues.
///
/// Leaking the `String` is acceptable here: `--version` is printed
/// once per process invocation and the memory is reclaimed on exit.
fn long_version() -> &'static str {
    Box::leak(
        format!(
            "{}\ntoolchain: {}\ncargo-dylint: {}",
            env!("CARGO_PKG_VERSION"),
            NIGHTLY_TOOLCHAIN,
            CARGO_DYLINT_VERSION
        )
        .into_boxed_str(),
    )
}

fn main() {
    let cli = Cli::parse();

    if cli.clear_cache {
        let cache_dir = cache::get_cache_dir(None);
        match cache::clear_cache(&cache_dir) {
            Ok(count) => {
                println!("Cleared {} cached lint result(s) from {:?}", count, cache_dir);
                exit(0);
            }
            Err(err) => {
                eprintln!("Error clearing cache: {}", err);
                exit(1);
            }
        }
    }

    if cli.list_lints {
        for name in LINT_NAMES {
            println!("{}", name);
        }
        exit(0);
    }

    if let Some(lint_name) = &cli.explain {
        if let Some(info) = LINT_INFOS.iter().find(|i| i.name == *lint_name) {
            println!("lint: {}", info.name);
            println!("category: {}", info.category);
            println!("severity: {}", info.severity);
            println!();
            println!("{}", info.description);
            if !info.rationale.is_empty() {
                println!();
                println!("Rationale:");
                println!("{}", info.rationale);
            }
            exit(0);
        } else if let Some(explanation) = LINT_EXPLANATIONS.get(lint_name.as_str()) {
            println!("{}", explanation);
            exit(0);
        } else {
            eprintln!("error: unknown lint '{}'", lint_name);
            exit(1);
        }
    }

    let budget_config_path = cli
        .config
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("budget.toml"));

    let budget_config = budget_config::parse_config(&budget_config_path);

    let known_lint_set = lint_name_set::build_lint_name_set(LINT_NAMES);
    let mut effective_allow = cli.allow.clone();
    let mut effective_warn = cli.warn.clone();
    let mut effective_deny = cli.deny.clone();

    if let Some(config_lints) = &budget_config.lints {
        for (lint, level) in config_lints {
            if !known_lint_set.contains(lint) {
                eprintln!("warning: unknown lint '{}' in budget.toml", lint);
                continue;
            }
            let normalized_lint = lint.to_lowercase();
            match level.as_str() {
                "allow" => {
                    if !effective_allow.contains(&normalized_lint) {
                        effective_allow.push(normalized_lint);
                    }
                }
                "warn" => {
                    if !effective_warn.contains(&normalized_lint) {
                        effective_warn.push(normalized_lint);
                    }
                }
                "deny" => {
                    if !effective_deny.contains(&normalized_lint) {
                        effective_deny.push(normalized_lint);
                    }
                }
                other => {
                    eprintln!(
                        "warning: unknown severity '{}' for lint '{}' in budget.toml",
                        other, lint
                    );
                }
            }
        }
    }

    let mut lint_flags = Vec::new();
    for lint in &effective_allow {
        lint_flags.push(format!("-A {}", lint));
    }
    for lint in &effective_warn {
        lint_flags.push(format!("-W {}", lint));
    }
    for lint in &effective_deny {
        lint_flags.push(format!("-D {}", lint));
    }

    let mut package_args = Vec::new();
    for pkg in &cli.package {
        package_args.push(format!("-p {}", pkg));
    }
    if cli.workspace {
        package_args.push(String::from("--workspace"));
    }

    let use_cache = !cli.no_cache;
    let cache_dir = cache::get_cache_dir(None);
    let cache_key_hash = if use_cache {
        match cache::compute_source_hash(Path::new(".")) {
            Ok(source_hash) => {
                let key = cache::CacheKey {
                    linter_version: env!("CARGO_PKG_VERSION").to_string(),
                    toolchain: cache::get_toolchain_version(),
                    lint_flags: lint_flags.clone(),
                    package_args: package_args.clone(),
                    output_format: format!("{:?}", cli.format),
                    source_hash,
                };
                Some(key.compute_hash())
            }
            Err(e) => {
                if cli.verbose {
                    eprintln!("warning: failed to compute source hash for cache: {}", e);
                }
                None
            }
        }
    } else {
        None
    };

    if use_cache {
        if let Some(ref hash) = cache_key_hash {
            if let Some(entry) = cache::load_cache_entry(&cache_dir, hash) {
                if cli.verbose {
                    eprintln!("[cache hit: {:?}]", hash);
                }
                if !entry.stdout.is_empty() {
                    print!("{}", entry.stdout);
                }
                if !entry.stderr.is_empty() {
                    eprintln!("{}", entry.stderr);
                }
                exit(entry.exit_code);
            }
        }
    }

    let mut cmd = Command::new("cargo");
    cmd.arg(format!("+{}", NIGHTLY_TOOLCHAIN));
    cmd.arg("dylint");
    cmd.arg("--workspace");

    for pkg in &cli.package {
        cmd.arg("-p");
        cmd.arg(pkg);
    }

    let effective_color = resolve_color_choice(&cli.color);
    if let Some(color_arg) = effective_color.as_cargo_arg() {
        cmd.arg("--color");
        cmd.arg(color_arg);
    }

    if cli.format == OutputFormat::Json {
        cmd.arg("--message-format=json");
    }

    cmd.env("DYLINT_LIB_NAME", "soroban_cost_lints");

    let mut rustflags = String::new();
    if !lint_flags.is_empty() {
        rustflags.push_str(&lint_flags.join(" "));
    }
    if let Ok(existing) = std::env::var("DYLINT_RUSTFLAGS") {
        if !rustflags.is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str(&existing);
    } else if let Ok(existing) = std::env::var("RUSTFLAGS") {
        if !rustflags.is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str(&existing);
    }

    if !rustflags.is_empty() {
        cmd.env("DYLINT_RUSTFLAGS", rustflags);
    }

    if cli.verbose {
        eprintln!("config path: {:?}", budget_config_path);
        eprintln!("lint flags: {:?}", lint_flags);
        eprintln!("spawned command: {:?}", cmd);
    }

    let show_progress = !cli.no_progress && std::io::stdout().is_terminal();
    let spinner = if show_progress {
        let pb = indicatif::ProgressBar::hidden();
        pb.set_style(
            indicatif::ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Running cargo cost-lint...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let exit_code;
    let mut captured_stdout = String::new();
    let mut captured_stderr = String::new();

    if cli.format == OutputFormat::Json {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                if let Some(ref pb) = spinner {
                    pb.finish_and_clear();
                }
                eprintln!("error: failed to spawn cargo dylint: {}", e);
                exit(1);
            }
        };

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        let mut findings = Vec::new();

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue,
            };

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(reason) = value.get("reason").and_then(|r| r.as_str()) {
                    if reason == "compiler-message" {
                        if let Some(msg) = value.get("message") {
                            if let Ok(finding) = serde_json::from_value::<LintFinding>(msg.clone()) {
                                findings.push(finding);
                                continue;
                            }
                        }
                    }
                }
            }

            captured_stdout.push_str(&line);
            captured_stdout.push('\n');
            println!("{}", line);
        }

        let status = match child.wait() {
            Ok(status) => status,
            Err(e) => {
                if let Some(ref pb) = spinner {
                    pb.finish_and_clear();
                }
                eprintln!("error: failed to wait for cargo dylint: {}", e);
                exit(1);
            }
        };

        if let Some(ref pb) = spinner {
            pb.finish_and_clear();
        }

        match cli.format {
            OutputFormat::Text => {
                for finding in &findings {
                    output_formatters::emit_text_finding(finding, &mut std::io::stdout()).unwrap();
                }
            }
            OutputFormat::Json => {
                for finding in &findings {
                    let json_str = serde_json::to_string(finding).unwrap();
                    captured_stdout.push_str(&json_str);
                    captured_stdout.push('\n');
                    println!("{}", json_str);
                }
            }
            OutputFormat::Sarif => {
                output_formatters::emit_sarif_report(&findings, &mut std::io::stdout()).unwrap();
            }
            OutputFormat::Github => {
                for finding in &findings {
                    output_formatters::emit_github_annotation(finding, &mut std::io::stdout()).unwrap();
                }
            }
        }

        exit_code = if status.success() { 0 } else { status.code().unwrap_or(1) };
    } else {
        let status = match cmd.status() {
            Ok(status) => status,
            Err(e) => {
                if let Some(ref pb) = spinner {
                    pb.finish_and_clear();
                }
                eprintln!("error: failed to execute cargo dylint: {}", e);
                exit(1);
            }
        };

        if let Some(ref pb) = spinner {
            pb.finish_and_clear();
        }

        exit_code = if status.success() { 0 } else { status.code().unwrap_or(1) };
    }

    if use_cache {
        if let Some(ref hash) = cache_key_hash {
            let entry = cache::CacheEntry {
                key: hash.clone(),
                exit_code,
                stdout: captured_stdout,
                stderr: captured_stderr,
            };
            let _ = cache::save_cache_entry(&cache_dir, hash, &entry);
        }
    }

    exit(exit_code);
}
