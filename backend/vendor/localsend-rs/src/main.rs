#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "https")]
    {
        use rustls::crypto::ring::default_provider;
        default_provider().install_default().ok();
    }

    use clap::Parser;
    #[cfg(feature = "tui")]
    use localsend_rs::cli::run_tui;
    use localsend_rs::cli::{Cli, Commands};
    use localsend_rs::cli::{run_discover, run_receive, run_send};

    let cli = Cli::parse();

    match cli.command {
        Commands::Discover(cmd) => {
            run_discover(cmd).await?;
        }
        Commands::Receive(cmd) => {
            run_receive(cmd).await?;
        }
        Commands::Send(cmd) => {
            run_send(cmd).await?;
        }
        #[cfg(feature = "tui")]
        Commands::Tui(cmd) => {
            run_tui(cmd).await?;
        }
    }

    Ok(())
}
