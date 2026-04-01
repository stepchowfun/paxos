mod acceptor;
mod config;
mod proposer;
mod rpc;
mod state;

#[macro_use]
extern crate log;

use {
    acceptor::acceptor,
    clap::{ArgAction, Parser},
    env_logger::{Builder, fmt::style::Effects},
    log::{Level, LevelFilter},
    proposer::propose,
    state::initial,
    std::{
        env,
        io::{self, Write},
        net::SocketAddr,
        path::PathBuf,
        process::exit,
        str::FromStr,
        string::ToString,
        sync::Arc,
        time::Duration,
    },
    tokio::{sync::RwLock, time::sleep, try_join},
};

// Defaults
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::Info;

// Duration constants
const PROPOSER_LOOP_DELAY: Duration = Duration::from_secs(1);

// This struct represents the raw command-line arguments.
#[derive(Parser)]
#[command(
    about = concat!(
        env!("CARGO_PKG_DESCRIPTION"),
        "\n\n",
        "More information can be found at: ",
        env!("CARGO_PKG_HOMEPAGE")
    ),
    version,
    disable_version_flag = true
)]
struct Cli {
    #[arg(short, long, help = "Print version", action = ArgAction::Version)]
    _version: Option<bool>,

    #[arg(
        short,
        long,
        value_name = "INDEX",
        help = "Set the index of the node corresponding to this instance",
        required = true
    )]
    node: String,

    #[arg(
        short = 'x',
        long,
        value_name = "VALUE",
        help = "Propose a value to the cluster"
    )]
    propose: Option<String>,

    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Set the path to the config file",
        default_value = "config.yml"
    )]
    config_file: PathBuf,

    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Set the path to the directory in which to store persistent data",
        default_value = "data"
    )]
    data_dir: PathBuf,

    #[arg(
        short,
        long,
        value_name = "ADDRESS",
        help = "Set the IP address to run on (if different from the configuration)"
    )]
    ip: Option<String>,

    #[arg(
        short,
        long,
        help = "Set the port to run on (if different from the configuration)"
    )]
    port: Option<String>,
}

// This struct represents the parsed command-line arguments.
#[derive(Clone)]
struct Settings {
    nodes: Vec<SocketAddr>,
    node_index: usize,
    address: SocketAddr,
    proposal: Option<String>,
    data_file_path: PathBuf,
}

// Set up the logger.
fn set_up_logging() {
    Builder::new()
        .filter_module(
            module_path!(),
            LevelFilter::from_str(
                &env::var("LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string()),
            )
            .unwrap_or(DEFAULT_LOG_LEVEL),
        )
        .format(|buf, record| {
            let level_for_style = match record.level() {
                Level::Trace => Level::Debug,
                level => level,
            };
            let style = buf
                .default_level_style(level_for_style)
                .effects(Effects::BOLD);
            let indent_size = record.level().to_string().len() + 3;
            let indent = &" ".repeat(indent_size);
            let options = textwrap::Options::with_termwidth()
                .initial_indent(indent)
                .subsequent_indent(indent);
            writeln!(
                buf,
                "{style}[{}]{style:#} {}",
                record.level(),
                &textwrap::fill(&record.args().to_string(), options)[indent_size..],
            )
        })
        .init();
}

// Parse the command-line options.
#[allow(clippy::too_many_lines)]
async fn settings() -> io::Result<Settings> {
    let cli = Cli::parse();

    // Parse the config file.
    let config = config::read(&cli.config_file).await?;

    // Parse the node index. Clap already guarantees that this required positional argument is
    // present.
    let node_repr = &cli.node;
    let node_index: usize = node_repr.parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{node_repr}` is not a valid node index. Reason: {error}"),
        )
    })?;
    if node_index >= config.nodes.len() {
        // [tag:node_index_valid]
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("There is no node with index {node_repr}."),
        ));
    }

    // Parse the IP address, if given.
    let ip = cli.ip.as_deref().map_or_else(
        || Ok(config.nodes[node_index].ip()), // [ref:node_index_valid]
        |raw_ip| {
            raw_ip.parse().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`{raw_ip}` is not a valid IP address. Reason: {error}"),
                )
            })
        },
    )?;

    // Parse the port number, if given.
    let port = cli.port.as_deref().map_or_else(
        || Ok(config.nodes[node_index].port()), // [ref:node_index_valid]
        |raw_port| {
            raw_port.parse().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`{raw_port}` is not a valid port number. Reason: {error}"),
                )
            })
        },
    )?;

    // Determine the data file path [tag:data_file_path_has_parent].
    let data_file_path = cli.data_dir.join(format!("{ip}-{port}"));

    // Return the settings.
    Ok(Settings {
        nodes: config.nodes,
        node_index,
        address: SocketAddr::new(ip, port),
        proposal: cli.propose,
        data_file_path,
    })
}

// Let the fun begin!
#[tokio::main]
async fn main() {
    // Set up the logger.
    set_up_logging();

    // Parse the command-line arguments.
    let settings = match settings().await {
        Ok(settings) => settings,
        Err(error) => {
            error!("{error}");
            exit(1);
        }
    };

    // Initialize the program state.
    let state = Arc::new(RwLock::new(initial()));

    // Attempt to read any persisted state.
    match state::read(&settings.data_file_path).await {
        Ok(durable_state) => {
            let mut guard = state.write().await;
            guard.0 = durable_state;
            info!("State loaded from persistent storage.");
        }
        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                info!("Starting from the initial state.");
            } else {
                error!(
                    "Unable to load state file `{}`. Reason: {}",
                    settings.data_file_path.to_string_lossy(),
                    error,
                );
                exit(1);
            }
        }
    }

    // Run the acceptor and the proposer. Even if there's no value to propose, we run the proposer
    // periodically to learn if a value was chosen and let the other nodes know about it.
    if let Err(error) = try_join!(
        acceptor(state.clone(), &settings.data_file_path, settings.address),
        async {
            loop {
                propose(
                    state.clone(),
                    &settings.data_file_path,
                    &settings.nodes,
                    settings.node_index,
                    settings.proposal.as_deref(),
                )
                .await?;

                if state.read().await.1.chosen_value.is_some() {
                    break;
                }

                sleep(PROPOSER_LOOP_DELAY).await;
            }

            Ok(())
        },
    ) {
        error!("{error}");
        exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
