use std::path::PathBuf;
use std::process::ExitCode;

use slacksitter_server::config::{Config, CONFIG_PATH_ENV, DEFAULT_CONFIG_PATH};
use slacksitter_server::{logging, run};

const USAGE: &str = "\
slacksitter-server: keeps a SQLite copy of Slack up to date and serves it over a Web API

USAGE:
    slacksitter-server [--config <path>]

OPTIONS:
    --config <path>   Settings file (TOML). Default: $SLACKSITTER_CONFIG, else ./slacksitter.toml
    -h, --help        Show this help
    -V, --version     Show the version

The Slack token and the API key can also come from SLACKSITTER__SLACK__TOKEN and
SLACKSITTER__SERVER__API_KEY. See SERVER.md for every setting.";

enum Args {
    Run { config_path: Option<PathBuf> },
    Exit,
}

fn parse_args() -> Result<Args, String> {
    let mut config_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(Args::Exit);
            }
            "-V" | "--version" => {
                println!("slacksitter-server {}", env!("CARGO_PKG_VERSION"));
                return Ok(Args::Exit);
            }
            "--config" | "-c" => {
                let value = args.next().ok_or("--config needs a path")?;
                config_path = Some(PathBuf::from(value));
            }
            other => match other.strip_prefix("--config=") {
                Some(value) => config_path = Some(PathBuf::from(value)),
                None => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
            },
        }
    }
    Ok(Args::Run { config_path })
}

#[tokio::main]
async fn main() -> ExitCode {
    let config_path = match parse_args() {
        Ok(Args::Run { config_path }) => config_path,
        Ok(Args::Exit) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let config = match Config::load(config_path.as_deref()) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("slacksitter-server: {err}");
            eprintln!(
                "(settings file: --config <path>, ${CONFIG_PATH_ENV}, or ./{DEFAULT_CONFIG_PATH}; \
                 see slacksitter.example.toml)"
            );
            return ExitCode::from(2);
        }
    };

    let _log_guard = logging::init(&config.log);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    match run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("fatal: {err}");
            eprintln!("slacksitter-server: {err}");
            ExitCode::FAILURE
        }
    }
}
