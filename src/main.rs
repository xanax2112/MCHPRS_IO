use clap::Parser;
use mchprs_core::server::{get_version_string, MinecraftServer};
use std::fs;
use std::path::Path;
use tracing::debug;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
struct Cli {
    /// Print version
    #[arg(short, long)]
    version: bool,
}

fn main() {
    let cli = Cli::parse();
    if cli.version {
        println!("{}", get_version_string());
        return;
    }

    // Setup logging
    let logfile = tracing_appender::rolling::daily("./logs", "mchprs.log");
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("MCHPRS_LOG")
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_writer(logfile.and(std::io::stdout))
        .with_env_filter(env_filter)
        .init();

    // Move old log file into logs folder
    let old_log_path = Path::new("./output.log");
    if old_log_path.exists() {
        let dest_path = "./logs/old_output.log";
        fs::rename(old_log_path, "./logs/old_output.log").unwrap();
        debug!(
            "Moving old log file from {old_log_path} to {dest_path}",
            old_log_path = old_log_path.display()
        );
    }

    // 插座 / 插头（BEDROCK.md 第 55 条）：默认构建没有基岩版，这里不执行任何东西；
    // `--features bedrock` 时把协议栈注册进 `mchprs_network::bedrock_api`，
    // 之后 core 通过 `bedrock_api::stack()` 就能用到它。
    #[cfg(feature = "bedrock")]
    mchprs_network::bedrock::stack::install();

    MinecraftServer::run();
}
