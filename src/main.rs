mod cli;
mod server;

pub mod calendar;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "service-report")]
#[command(about = "Calendar service report tool", long_about = None)]
struct AppArgs {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true, default_value = "calendar-config.yaml")]
    calendar_config: String,

    #[arg(long, global = true, default_value = "rates.yaml")]
    rates_config: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Generate a report from Google Calendar")]
    Report {
        #[arg(long)]
        event_prefix: Option<String>,

        #[arg(long)]
        month: u32,

        #[arg(long)]
        year: i32,
    },

    #[command(about = "Start the HTTP server")]
    Serve {
        #[arg(long)]
        event_prefix: Option<String>,

        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value = "8000")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = AppArgs::parse();

    match args.command {
        Commands::Report {
            event_prefix,
            month,
            year,
        } => {
            let calendar_config = cli::load_config(&args.calendar_config)?;
            let rates_config = cli::load_rates(&args.rates_config)?;
            cli::run_report(
                &calendar_config,
                &rates_config,
                event_prefix.as_deref(),
                month,
                year,
            )?;
        }
        Commands::Serve {
            event_prefix,
            host,
            port,
        } => {
            server::run(
                event_prefix,
                host,
                port,
                args.calendar_config,
                args.rates_config,
            )
            .await?;
        }
    }

    Ok(())
}
