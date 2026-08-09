//! TUI command for interactive terminal interface.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tui", about = "Launch interactive TUI mode")]
pub struct TuiCommand {
    /// Port to listen on
    #[arg(short, long, default_value = "53317")]
    pub port: u16,

    /// Device alias name
    #[arg(short, long)]
    pub alias: Option<String>,

    /// Require this PIN from senders before a transfer is offered.
    #[arg(long)]
    pub pin: Option<String>,

    /// Accept every incoming transfer without prompting.
    #[arg(long)]
    pub auto_accept: bool,

    /// Use plain HTTP instead of HTTPS. LocalSend uses HTTPS by default (matching
    /// the official app); pass this for easy interop/testing with HTTP-only peers.
    #[cfg(feature = "https")]
    #[arg(long)]
    pub no_https: bool,
}

pub async fn execute(command: TuiCommand) -> anyhow::Result<()> {
    #[cfg(feature = "https")]
    let https = !command.no_https;
    #[cfg(not(feature = "https"))]
    let https = false;

    crate::tui::run_tui(
        command.port,
        command.alias,
        https,
        command.pin,
        command.auto_accept,
    )
    .await
    .map_err(|e| anyhow::anyhow!("TUI error: {}", e))
}

#[cfg(all(test, feature = "https"))]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn defaults_to_https() {
        let cmd = TuiCommand::try_parse_from(["tui"]).unwrap();
        assert!(
            !cmd.no_https,
            "TUI must default to HTTPS (no_https = false)"
        );
    }

    #[test]
    fn no_https_flag_opts_out() {
        let cmd = TuiCommand::try_parse_from(["tui", "--no-https"]).unwrap();
        assert!(cmd.no_https);
    }
}
