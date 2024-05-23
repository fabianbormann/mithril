#![doc = include_str!("../README.md")]

use anyhow::{anyhow, Context};
use clap::builder::Styles;
use clap::{ArgMatches, CommandFactory, Parser, Subcommand};
use config::{builder::DefaultState, ConfigBuilder, Map, Source, Value, ValueKind};
use slog::{Drain, Fuse, Level, Logger};
use slog_async::Async;
use slog_scope::debug;
use slog_term::Decorator;
use std::io::Write;
use std::sync::Arc;
use std::{fs::File, path::PathBuf};

use mithril_client::MithrilResult;
use mithril_doc::{Documenter, GenerateDocCommands, StructDoc};

use mithril_client_cli::commands::{
    cardano_db::{deprecated::SnapshotCommands, CardanoDbCommands},
    cardano_transaction::CardanoTransactionCommands,
    mithril_stake_distribution::MithrilStakeDistributionCommands,
};

use clap::{
    builder::StyledStr,
    error::{ContextKind, ContextValue, ErrorKind},
    FromArgMatches,
};

enum LogOutputType {
    StdErr,
    File(String),
}

impl LogOutputType {
    fn get_writer(&self) -> MithrilResult<Box<dyn Write + Send>> {
        let writer: Box<dyn Write + Send> = match self {
            LogOutputType::StdErr => Box::new(std::io::stderr()),
            LogOutputType::File(filepath) => Box::new(
                File::create(filepath)
                    .with_context(|| format!("Can not create output log file: {}", filepath))?,
            ),
        };

        Ok(writer)
    }
}

#[derive(Documenter, Parser, Debug, Clone)]
#[clap(name = "mithril-client")]
#[clap(
about = "This program shows, downloads and verifies certified blockchain artifacts.",
long_about = None
)]
#[command(version)]
pub struct Args {
    /// Available commands
    #[clap(subcommand)]
    command: ArtifactCommands,

    /// Run Mode.
    #[clap(long, env = "RUN_MODE", default_value = "dev")]
    run_mode: String,

    /// Verbosity level (-v=warning, -vv=info, -vvv=debug).
    #[clap(short, long, action = clap::ArgAction::Count)]
    #[example = "Parsed from the number of occurrences: `-v` for `Warning`, `-vv` for `Info`, `-vvv` for `Debug` and `-vvvv` for `Trace`"]
    verbose: u8,

    /// Directory where configuration file is located.
    #[clap(long, default_value = "./config")]
    pub config_directory: PathBuf,

    /// Override configuration Aggregator endpoint URL.
    #[clap(long, env = "AGGREGATOR_ENDPOINT")]
    #[example = "`https://aggregator.pre-release-preview.api.mithril.network/aggregator`"]
    aggregator_endpoint: Option<String>,

    /// Enable JSON output for logs displayed according to verbosity level
    #[clap(long)]
    log_format_json: bool,

    /// Redirect the logs to a file
    #[clap(long, alias("o"))]
    #[example = "`./mithril-client.log`"]
    log_output: Option<String>,

    /// Enable unstable commands (such as Cardano Transactions)
    #[clap(long)]
    unstable: bool,
}

impl Args {
    pub async fn execute(&self) -> MithrilResult<()> {
        debug!("Run Mode: {}", self.run_mode);
        let filename = format!("{}/{}.json", self.config_directory.display(), self.run_mode);
        debug!("Reading configuration file '{}'.", filename);
        let config: ConfigBuilder<DefaultState> = config::Config::builder()
            .add_source(config::File::with_name(&filename).required(false))
            .add_source(self.clone())
            .set_default("download_dir", "")?;

        self.command.execute(self.unstable, config).await
    }

    fn log_level(&self) -> Level {
        match self.verbose {
            0 => Level::Error,
            1 => Level::Warning,
            2 => Level::Info,
            3 => Level::Debug,
            _ => Level::Trace,
        }
    }

    fn get_log_output_type(&self) -> LogOutputType {
        if let Some(output_filepath) = &self.log_output {
            LogOutputType::File(output_filepath.to_string())
        } else {
            LogOutputType::StdErr
        }
    }

    fn wrap_drain<D: Decorator + Send + 'static>(&self, decorator: D) -> Fuse<Async> {
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog::LevelFilter::new(drain, self.log_level()).fuse();

        slog_async::Async::new(drain).build().fuse()
    }

    fn build_logger(&self) -> MithrilResult<Logger> {
        let log_output_type = self.get_log_output_type();
        let writer = log_output_type.get_writer()?;

        let drain = if self.log_format_json {
            let drain = slog_bunyan::new(writer).set_pretty(false).build().fuse();
            let drain = slog::LevelFilter::new(drain, self.log_level()).fuse();

            slog_async::Async::new(drain).build().fuse()
        } else {
            match log_output_type {
                LogOutputType::StdErr => self.wrap_drain(slog_term::TermDecorator::new().build()),
                LogOutputType::File(_) => self.wrap_drain(slog_term::PlainDecorator::new(writer)),
            }
        };

        Ok(Logger::root(Arc::new(drain), slog::o!()))
    }

    fn parse_deprecated_XXXX() -> Self {
        let styles = Self::command().get_styles().clone();
        let result = handle_deprecated(Self::try_parse(), styles);
        match result {
            Ok(s) => s,
            Err(e) => {
                // Since this is more of a development-time error, we aren't doing as fancy of a quit
                // as `get_matches`
                e.exit()
            }
        }
    }
}

impl Source for Args {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, config::ConfigError> {
        let mut map = Map::new();
        let namespace = "clap arguments".to_string();

        if let Some(aggregator_endpoint) = self.aggregator_endpoint.clone() {
            map.insert(
                "aggregator_endpoint".to_string(),
                Value::new(Some(&namespace), ValueKind::from(aggregator_endpoint)),
            );
        }

        Ok(map)
    }
}

#[derive(Subcommand, Debug, Clone)]
enum ArtifactCommands {
    // /// Deprecated, use `cardano-db` instead
    // #[clap(subcommand)]
    // #[deprecated(since = "0.7.3", note = "use `CardanoDb` commands instead")]
    // Snapshot(SnapshotCommands),
    #[clap(subcommand, alias("cdb"))]
    CardanoDb(CardanoDbCommands),

    #[clap(subcommand, alias("msd"))]
    MithrilStakeDistribution(MithrilStakeDistributionCommands),

    #[clap(subcommand, alias("ctx"))]
    CardanoTransaction(CardanoTransactionCommands),

    #[clap(alias("doc"), hide(true))]
    GenerateDoc(GenerateDocCommands),
}

impl ArtifactCommands {
    pub async fn execute(
        &self,
        unstable_enabled: bool,
        config_builder: ConfigBuilder<DefaultState>,
    ) -> MithrilResult<()> {
        match self {
            // #[allow(deprecated)]
            // Self::Snapshot(cmd) => {
            //     let message = "`snapshot` command is deprecated, use `cardano-db` instead";
            //     if cmd.is_json_output_enabled() {
            //         eprintln!(r#"{{"warning": "{}", "type": "deprecation"}}"#, message);
            //     } else {
            //         eprintln!("{}", message);
            //     };
            //     cmd.execute(config_builder).await
            // }
            Self::CardanoDb(cmd) => cmd.execute(config_builder).await,
            Self::MithrilStakeDistribution(cmd) => cmd.execute(config_builder).await,
            Self::CardanoTransaction(ctx) => {
                if !unstable_enabled {
                    Err(anyhow::anyhow!(
                        "The \"cardano-transaction\" subcommand is only accepted using the \
                        --unstable flag.\n \
                    \n \
                    ie: \"mithril-client --unstable cardano-transaction list\""
                    ))
                } else {
                    ctx.execute(config_builder).await
                }
            }
            Self::GenerateDoc(cmd) => cmd
                .execute(&mut Args::command())
                .map_err(|message| anyhow!(message)),
        }
    }
}

#[tokio::main]
async fn main() -> MithrilResult<()> {
    // Load args
    let args = Args::parse_deprecated_XXXX();
    let _guard = slog_scope::set_global_logger(args.build_logger()?);

    #[cfg(feature = "bundle_openssl")]
    openssl_probe::init_ssl_cert_env_vars();

    args.execute().await
}

struct DeprecatedCommand {
    command: String,
    new_command: String,
}

fn handle_deprecated_commands<A>(
    matches_result: Result<A, clap::error::Error>,
    styles: Styles,
    deprecated_commands: Vec<DeprecatedCommand>,
) -> Result<A, clap::error::Error> {
    matches_result.map_err(|mut e: clap::error::Error| {
        fn get_deprecated_command(
            error: &clap::error::Error,
            deprecated_commands: Vec<DeprecatedCommand>,
        ) -> Option<DeprecatedCommand> {
            if let Some(context_value) = error.get(ContextKind::InvalidSubcommand) {
                let command = context_value.to_string();
                for deprecated_command in deprecated_commands {
                    if command == deprecated_command.command {
                        return Some(deprecated_command);
                    }
                }
            }
            None
        }
        if let Some(deprecated_command) = get_deprecated_command(&e, deprecated_commands) {
            // let message = match styles {
            //     None => format!(
            //         "'{}' command is deprecated, use '{}' command instead",
            //         deprecated_command.command, deprecated_command.new_command,
            //     ),
            //     Some(s) => format!(
            //         "'{}{}{}' command is deprecated, use '{}{}{}' command instead",
            //         s.get_error().render(),
            //         deprecated_command.command,
            //         s.get_error().render_reset(),
            //         s.get_valid().render(),
            //         deprecated_command.new_command,
            //         s.get_valid().render_reset(),
            //     ),
            // };
            let message = format!(
                "'{}{}{}' command is deprecated, use '{}{}{}' command instead",
                styles.get_error().render(),
                deprecated_command.command,
                styles.get_error().render_reset(),
                styles.get_valid().render(),
                deprecated_command.new_command,
                styles.get_valid().render_reset(),
            );
            e.insert(
                ContextKind::Suggested,
                ContextValue::StyledStrs(vec![StyledStr::from(&message)]),
            );
        }
        e
    })
}

fn handle_deprecated<A>(
    matches_result: Result<A, clap::error::Error>,
    styles: Styles,
) -> Result<A, clap::error::Error> {
    handle_deprecated_commands(
        matches_result,
        styles,
        vec![DeprecatedCommand {
            command: "snapshot".to_string(),
            new_command: "cardano-db".to_string(),
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // use clap::{CommandFactory, FromArgMatches};
    use clap::{
        builder::StyledStr,
        error::{ContextKind, ContextValue, ErrorKind},
        FromArgMatches,
    };

    #[derive(Documenter, Parser, Debug, Clone)]
    #[clap(name = "mithril-client")]
    #[clap(
    about = "This program shows, downloads and verifies certified blockchain artifacts.",
    long_about = None
    )]
    #[command(version)]
    pub struct MyCmd {
        /// Available commands
        #[clap(subcommand)]
        command: ArtifactCommands,
    }

    impl MyCmd {
        pub async fn execute(&self) -> MithrilResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn fail_if_cardano_tx_command_is_used_without_unstable_flag() {
        let args =
            Args::try_parse_from(["mithril-client", "cardano-transaction", "snapshot", "list"])
                .unwrap();

        args.execute()
            .await
            .expect_err("Should fail if unstable flag missing");
    }

    #[test]
    fn XXXX_cardano_db_is_a_valid_command() {
        let command_line = ["", "cardano-db", "snapshot", "list"];
        let matches_result = MyCmd::command().try_get_matches_from_mut(&command_line);
        let result = handle_deprecated(matches_result, Styles::plain());
        assert!(result.is_ok());
    }

    #[test]
    fn XXXX_snapshot_is_not_anymore_a_command() {
        let command_line = ["", "snapshot", "list"];
        let matches_result = MyCmd::command().try_get_matches_from_mut(&command_line);
        let result = handle_deprecated(matches_result, Styles::plain());

        assert!(result.is_err());
        let message = result.err().unwrap().to_string();
        //TODO to remove
        println!("Error message: ---\n{message}\n---");
        assert!(message.contains("'snapshot'"));
        assert!(message.contains("'cardano-db'"));
    }

    #[test]
    fn XXXX_show_deprecated_message_only_with_specific_commands() {
        let command_line = ["", "unknown_not_deprecated", "list"];
        let matches_result = MyCmd::command().try_get_matches_from_mut(&command_line);
        let result = handle_deprecated(matches_result, Styles::plain());

        assert!(result.is_err());
        let message = result.err().unwrap().to_string();
        assert!(message.contains("'unknown_not_deprecated'"));
        assert!(!message.contains("'cardano-db'"));
    }

    #[test]
    fn XXXX_replace_error_message_on_deprecated_commands() {
        {
            let mut e = clap::error::Error::new(clap::error::ErrorKind::InvalidSubcommand)
                .with_cmd(&MyCmd::command());
            e.insert(
                ContextKind::InvalidSubcommand,
                ContextValue::String("deprecated_command".to_string()),
            );
            let result = handle_deprecated_commands(
                Err(e) as Result<MyCmd, clap::error::Error>,
                Styles::plain(),
                vec![DeprecatedCommand {
                    command: "deprecated_other_command".to_string(),
                    new_command: "new_command".to_string(),
                }],
            );
            assert!(result.is_err());
            let message = result.err().unwrap().to_string();
            assert!(message.contains("'deprecated_command'"));
            assert!(!message.contains("'new_command'"));
        }
        {
            let mut e = clap::error::Error::new(clap::error::ErrorKind::InvalidSubcommand)
                .with_cmd(&MyCmd::command());
            e.insert(
                ContextKind::InvalidSubcommand,
                ContextValue::String("deprecated_command".to_string()),
            );

            let result = handle_deprecated_commands(
                Err(e) as Result<MyCmd, clap::error::Error>,
                Styles::plain(),
                vec![DeprecatedCommand {
                    command: "deprecated_command".to_string(),
                    new_command: "new_command".to_string(),
                }],
            );
            assert!(result.is_err());
            let message = result.err().unwrap().to_string();
            assert!(message.contains("'deprecated_command'"));
            assert!(message.contains("'new_command'"));
        }
    }
}
