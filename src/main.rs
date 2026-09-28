use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APP_NAME: &str = "Smart Command Runner";
const BIN_NAME: &str = "cmdrun";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Alexander Suvorov";
const GITHUB: &str = "smartlegionlab";
const REPO_URL: &str = "https://github.com/smartlegionlab/smart-command-runner-rs";

#[derive(Parser, Debug)]
#[command(
    name = BIN_NAME,
    version = VERSION,
    author = AUTHOR,
    about = "Run grouped shell commands from a TOML file.",
    long_about = "Smart Command Runner reads a TOML file with named blocks of shell commands \
and runs them. Blocks can be selected individually, all at once, or with per-block / per-command \
confirmation. Dry-run by default — nothing runs unless --yes, --interactive-block, or \
--interactive-command is given.",
    after_help = "Repository: https://github.com/smartlegionlab/smart-command-runner-rs"
)]
struct Cli {
    #[arg(
        short = 'f',
        long = "file",
        value_name = "PATH",
        help = "TOML file with blocks of commands (required)"
    )]
    file: PathBuf,

    #[arg(
        short = 'r',
        long = "run",
        value_name = "LIST",
        value_delimiter = ',',
        help = "Run only these blocks, in this order (comma-separated)"
    )]
    run: Vec<String>,

    #[arg(long = "list", help = "List blocks in the file and exit")]
    list: bool,

    #[arg(short = 'y', long = "yes", help = "Run everything without asking")]
    yes: bool,

    #[arg(
        short = 'i',
        long = "interactive-block",
        help = "Ask confirmation per block"
    )]
    interactive_block: bool,

    #[arg(
        short = 'c',
        long = "interactive-command",
        help = "Ask confirmation per command"
    )]
    interactive_command: bool,

    #[arg(long = "dry-run", help = "Force dry-run even with --yes")]
    dry_run: bool,

    #[arg(long = "log", value_name = "PATH", help = "Log file path")]
    log: Option<PathBuf>,

    #[arg(long = "no-log", help = "Do not write a log file")]
    no_log: bool,

    #[arg(
        long = "sh",
        value_name = "PATH",
        default_value = "sh",
        help = "Shell used to run each command (with -c)"
    )]
    sh: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Block {
    #[serde(default)]
    description: String,
    commands: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(flatten)]
    blocks: HashMap<String, Block>,
}

#[derive(Debug, Default)]
struct RunSummary {
    commands_run: u64,
    commands_skipped: u64,
    commands_failed: u64,
    blocks_run: u64,
    blocks_skipped: u64,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmChoice {
    Yes,
    No,
    All,
    Quit,
}

fn parse_confirm(input: &str) -> ConfirmChoice {
    let s = input.trim().to_lowercase();
    match s.as_str() {
        "y" | "yes" => ConfirmChoice::Yes,
        "a" | "all" => ConfirmChoice::All,
        "q" | "quit" => ConfirmChoice::Quit,
        _ => ConfirmChoice::No,
    }
}

fn ask_block(name: &str, description: &str, count: usize) -> ConfirmChoice {
    if !description.is_empty() {
        eprintln!("Block [{}] — {}", name, description);
    } else {
        eprintln!("Block [{}]", name);
    }
    eprintln!("  Commands: {}", count);
    eprint!("Run this block? [y/N/a/q] ");
    std::io::stderr().flush().ok();

    let mut input = String::new();
    match std::io::stdin().lock().read_line(&mut input) {
        Ok(0) => ConfirmChoice::Quit,
        Ok(_) => parse_confirm(&input),
        Err(_) => ConfirmChoice::Quit,
    }
}

fn ask_command(index: usize, total: usize, name: &str, command: &str) -> ConfirmChoice {
    eprint!("  [{}/{}] [{}] {}? [y/N/a/q] ", index, total, name, command);
    std::io::stderr().flush().ok();

    let mut input = String::new();
    match std::io::stdin().lock().read_line(&mut input) {
        Ok(0) => ConfirmChoice::Quit,
        Ok(_) => parse_confirm(&input),
        Err(_) => ConfirmChoice::Quit,
    }
}

fn current_year() -> u64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut year = 1970u64;
    let mut remaining = secs;

    loop {
        let days = if is_leap_year(year) { 366 } else { 365 };
        let year_secs = days * 86_400;
        if remaining < year_secs {
            break;
        }
        remaining -= year_secs;
        year += 1;
    }

    year
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn iso_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut year = 1970u64;
    let mut remaining = secs;
    loop {
        let days = if is_leap_year(year) { 366 } else { 365 };
        let year_secs = days * 86_400;
        if remaining < year_secs {
            break;
        }
        remaining -= year_secs;
        year += 1;
    }
    let day_of_year = remaining / 86_400;
    let mut month = 1u64;
    let mut day = day_of_year + 1;
    let month_days = |m: u64, y: u64| -> u64 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if is_leap_year(y) {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    };
    while day > month_days(month, year) {
        day -= month_days(month, year);
        month += 1;
    }
    let time_secs = remaining % 86_400;
    let hh = time_secs / 3600;
    let mm = (time_secs % 3600) / 60;
    let ss = time_secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hh, mm, ss
    )
}

fn default_log_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
            .join(".local/share/cmdrun")
            .join("cmdrun.log")
    } else {
        PathBuf::from("/tmp/cmdrun.log")
    }
}

fn print_header() {
    println!("{} v{}", APP_NAME, VERSION);
    println!();
}

fn print_footer() {
    let year = current_year();
    println!();
    println!("Copyright (c) {} {} <{}>", year, AUTHOR, GITHUB);
    println!("Repo: {}", REPO_URL);
}

fn load_config(path: &Path) -> Result<Config, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    toml::from_str(&content).map_err(|e| format!("cannot parse {}: {}", path.display(), e))
}

fn ordered_block_names(config: &Config) -> Vec<String> {
    let content_names: Vec<String> = config.blocks.keys().cloned().collect();
    let mut sorted = content_names;
    sorted.sort();
    sorted
}

fn select_blocks(config: &Config, requested: &[String]) -> Result<Vec<String>, String> {
    if requested.is_empty() {
        return Ok(ordered_block_names(config));
    }
    let mut result = Vec::new();
    for name in requested {
        if !config.blocks.contains_key(name) {
            return Err(format!("block '{}' not found in file", name));
        }
        result.push(name.clone());
    }
    Ok(result)
}

fn run_command(sh: &Path, command: &str) -> Result<(u64, String), String> {
    let output = Command::new(sh)
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to launch shell: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut combined = stdout;
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }

    if output.status.success() {
        Ok((0, combined))
    } else {
        let code = output.status.code().unwrap_or(-1) as u64;
        Err(format!("exit {}: {}", code, combined.trim()))
    }
}

fn open_log(path: &Path) -> Option<File> {
    if let Some(parent) = path.parent() {
        if let Err(_) = fs::create_dir_all(parent) {
            return None;
        }
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn log_line(log: &mut Option<File>, line: &str) {
    if let Some(f) = log.as_mut() {
        let _ = writeln!(f, "{}", line);
    }
}

fn print_plan(config: &Config, blocks: &[String]) {
    println!("Blocks to run:");
    println!();
    for (i, name) in blocks.iter().enumerate() {
        let block = config.blocks.get(name).unwrap();
        let desc = if block.description.is_empty() {
            String::new()
        } else {
            format!(" — {}", block.description)
        };
        println!(
            "  [{}] {}{} ({} commands)",
            i + 1,
            name,
            desc,
            block.commands.len()
        );
        for (j, cmd) in block.commands.iter().enumerate() {
            println!("      {}. {}", j + 1, cmd);
        }
    }
    println!();
    println!("Dry-run: no commands were executed.");
    println!("To execute, add --yes, --interactive-block, or --interactive-command.");
}

fn execute(cli: &Cli, config: &Config, blocks: &[String], log: &mut Option<File>) -> RunSummary {
    let mut summary = RunSummary::default();
    let mut all_blocks = false;
    let mut all_commands = false;
    let mut total_commands: u64 = blocks
        .iter()
        .map(|n| {
            config
                .blocks
                .get(n)
                .map(|b| b.commands.len() as u64)
                .unwrap_or(0)
        })
        .sum();

    let pb = if cli.interactive_block || cli.interactive_command {
        None
    } else {
        let pb = ProgressBar::new(total_commands);
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    };

    for block_name in blocks {
        let block = match config.blocks.get(block_name) {
            Some(b) => b,
            None => continue,
        };

        if cli.interactive_block && !all_blocks {
            match ask_block(block_name, &block.description, block.commands.len()) {
                ConfirmChoice::Yes => {}
                ConfirmChoice::No => {
                    summary.blocks_skipped += 1;
                    summary.commands_skipped += block.commands.len() as u64;
                    if let Some(ref pb) = pb {
                        pb.inc(block.commands.len() as u64);
                    }
                    continue;
                }
                ConfirmChoice::All => {
                    all_blocks = true;
                }
                ConfirmChoice::Quit => {
                    eprintln!("Interrupted by user.");
                    break;
                }
            }
        }

        let mut block_commands_ok: u64 = 0;
        let block_start = Instant::now();

        for (i, command) in block.commands.iter().enumerate() {
            if cli.interactive_command && !all_commands {
                match ask_command(i + 1, block.commands.len(), block_name, command) {
                    ConfirmChoice::Yes => {}
                    ConfirmChoice::No => {
                        summary.commands_skipped += 1;
                        if let Some(ref pb) = pb {
                            pb.inc(1);
                        }
                        continue;
                    }
                    ConfirmChoice::All => {
                        all_commands = true;
                    }
                    ConfirmChoice::Quit => {
                        eprintln!("Interrupted by user.");
                        if let Some(ref pb) = pb {
                            pb.finish_and_clear();
                        }
                        summary.blocks_run += 1;
                        return summary;
                    }
                }
            }

            let ts = iso_timestamp();
            log_line(log, &format!("[{}] [{}] RUN: {}", ts, block_name, command));

            println!("[{}] $ {}", block_name, command);
            let cmd_start = Instant::now();
            match run_command(&cli.sh, command) {
                Ok((_, _out)) => {
                    let elapsed = cmd_start.elapsed().as_secs_f64();
                    println!("[{}] OK ({:.2}s)", block_name, elapsed);
                    log_line(
                        log,
                        &format!(
                            "[{}] [{}] OK ({:.2}s)",
                            iso_timestamp(),
                            block_name,
                            elapsed
                        ),
                    );
                    summary.commands_run += 1;
                    block_commands_ok += 1;
                }
                Err(e) => {
                    let elapsed = cmd_start.elapsed().as_secs_f64();
                    eprintln!("[{}] FAIL ({:.2}s): {}", block_name, elapsed, e);
                    log_line(
                        log,
                        &format!(
                            "[{}] [{}] FAIL ({:.2}s): {}",
                            iso_timestamp(),
                            block_name,
                            elapsed,
                            e
                        ),
                    );
                    summary.commands_failed += 1;
                    summary
                        .errors
                        .push(format!("[{}] {}: {}", block_name, command, e));
                }
            }

            if let Some(ref pb) = pb {
                pb.inc(1);
            }
        }

        if block_commands_ok > 0 || block.commands.is_empty() {
            summary.blocks_run += 1;
        }
        let block_elapsed = block_start.elapsed().as_secs_f64();
        if cli.interactive_block || cli.interactive_command {
            println!("[{}] Block done ({:.2}s)", block_name, block_elapsed);
        }
        log_line(
            log,
            &format!(
                "[{}] [{}] BLOCK DONE ({:.2}s)",
                iso_timestamp(),
                block_name,
                block_elapsed
            ),
        );
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    let _ = total_commands;
    total_commands = 0;
    let _ = total_commands;
    summary
}

fn main() {
    let cli = Cli::parse();

    if cli.yes && (cli.interactive_block || cli.interactive_command) {
        eprintln!(
            "Error: --yes cannot be combined with --interactive-block or --interactive-command"
        );
        std::process::exit(1);
    }

    if (cli.interactive_block || cli.interactive_command) && !std::io::stdin().is_terminal() {
        eprintln!(
            "Error: --interactive-block/--interactive-command require a TTY (stdin is not a terminal)"
        );
        std::process::exit(1);
    }

    if !cli.file.exists() {
        eprintln!("Error: file does not exist: {}", cli.file.display());
        std::process::exit(1);
    }

    let config = match load_config(&cli.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if config.blocks.is_empty() {
        eprintln!("Error: no blocks found in {}", cli.file.display());
        std::process::exit(1);
    }

    let blocks = match select_blocks(&config, &cli.run) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if cli.list {
        println!("Blocks in {}:", cli.file.display());
        println!();
        for name in &blocks {
            let block = config.blocks.get(name).unwrap();
            let desc = if block.description.is_empty() {
                String::new()
            } else {
                format!(" — {}", block.description)
            };
            println!("  {}{} ({} commands)", name, desc, block.commands.len());
        }
        std::process::exit(0);
    }

    print_header();

    println!("File:   {}", cli.file.display());
    println!("Blocks: {}", blocks.len());
    let total_commands: usize = blocks
        .iter()
        .map(|n| config.blocks.get(n).map(|b| b.commands.len()).unwrap_or(0))
        .sum();
    println!("Commands total: {}", total_commands);
    println!();

    let dry_run = cli.dry_run || (!cli.yes && !cli.interactive_block && !cli.interactive_command);

    if dry_run {
        print_plan(&config, &blocks);
        print_footer();
        return;
    }

    let log_path = if cli.no_log {
        None
    } else {
        Some(cli.log.clone().unwrap_or_else(default_log_path))
    };

    let mut log_file = log_path.as_ref().and_then(|p| open_log(p));
    log_line(
        &mut log_file,
        &format!(
            "[{}] === cmdrun START file={} blocks={}",
            iso_timestamp(),
            cli.file.display(),
            blocks.join(",")
        ),
    );

    let summary = execute(&cli, &config, &blocks, &mut log_file);

    println!();
    println!("Blocks run:     {}", summary.blocks_run);
    println!("Blocks skipped: {}", summary.blocks_skipped);
    println!("Commands run:   {}", summary.commands_run);
    println!("Commands skip:  {}", summary.commands_skipped);
    println!("Commands fail:  {}", summary.commands_failed);

    if !summary.errors.is_empty() {
        println!();
        println!("Errors:");
        for e in summary.errors.iter().take(20) {
            println!("  {}", e);
        }
        if summary.errors.len() > 20 {
            println!("  ... and {} more", summary.errors.len() - 20);
        }
    }

    if let Some(p) = &log_path {
        println!();
        println!("Log: {}", p.display());
    }

    log_line(
        &mut log_file,
        &format!(
            "[{}] === cmdrun END run={} skipped={} failed={}",
            iso_timestamp(),
            summary.commands_run,
            summary.commands_skipped,
            summary.commands_failed
        ),
    );

    print_footer();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_confirm_yes() {
        assert_eq!(parse_confirm("y"), ConfirmChoice::Yes);
        assert_eq!(parse_confirm("Y"), ConfirmChoice::Yes);
        assert_eq!(parse_confirm("yes"), ConfirmChoice::Yes);
        assert_eq!(parse_confirm("  YES  "), ConfirmChoice::Yes);
    }

    #[test]
    fn test_parse_confirm_no() {
        assert_eq!(parse_confirm("n"), ConfirmChoice::No);
        assert_eq!(parse_confirm(""), ConfirmChoice::No);
        assert_eq!(parse_confirm("garbage"), ConfirmChoice::No);
    }

    #[test]
    fn test_parse_confirm_all() {
        assert_eq!(parse_confirm("a"), ConfirmChoice::All);
        assert_eq!(parse_confirm("ALL"), ConfirmChoice::All);
    }

    #[test]
    fn test_parse_confirm_quit() {
        assert_eq!(parse_confirm("q"), ConfirmChoice::Quit);
        assert_eq!(parse_confirm("quit"), ConfirmChoice::Quit);
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1900));
    }

    #[test]
    fn test_load_config_simple() {
        let toml = r#"
[base]
description = "Common setup"
commands = [
    "echo hello",
    "echo world",
]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.blocks.len(), 1);
        let base = config.blocks.get("base").unwrap();
        assert_eq!(base.commands.len(), 2);
        assert_eq!(base.description, "Common setup");
    }

    #[test]
    fn test_select_blocks_all() {
        let toml = r#"
[beta]
commands = ["echo b"]
[alpha]
commands = ["echo a"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let selected = select_blocks(&config, &[]).unwrap();
        assert_eq!(selected, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_select_blocks_named() {
        let toml = r#"
[base]
commands = ["echo b"]
[manjaro]
commands = ["echo m"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let selected =
            select_blocks(&config, &["base".to_string(), "manjaro".to_string()]).unwrap();
        assert_eq!(selected, vec!["base", "manjaro"]);
    }

    #[test]
    fn test_select_blocks_missing() {
        let toml = r#"
[base]
commands = ["echo b"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let result = select_blocks(&config, &["nope".to_string()]);
        assert!(result.is_err());
    }
}
