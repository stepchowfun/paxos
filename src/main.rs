#![deny(clippy::all, clippy::pedantic, warnings)]

mod acceptor;
mod config;
mod proposer;
mod state;

#[macro_use]
extern crate log;

use {
    clap::{App, AppSettings, Arg},
    env_logger::{fmt::Color, Builder},
    log::{Level, LevelFilter},
    proposer::propose,
    state::initial,
    std::{
        env,
        io::{self, Write},
        net::SocketAddr,
        path::{Path, PathBuf},
        process::exit,
        str::FromStr,
        string::ToString,
        sync::Arc,
    },
    tokio::{sync::RwLock, try_join},
};

// The program version
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Defaults
const CONFIG_FILE_DEFAULT_PATH: &str = "config.yml";
const DATA_DIR_DEFAULT_PATH: &str = "data";
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::Info;

// Command-line option names
const CONFIG_FILE_OPTION: &str = "config-file";
const DATA_DIR_OPTION: &str = "data-dir";
const IP_OPTION: &str = "ip";
const NODE_OPTION: &str = "node";
const PORT_OPTION: &str = "port";
const PROPOSE_OPTION: &str = "propose";

// This struct represents a summary of the command-line options
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
            let mut style = buf.style();
            style.set_bold(true);
            match record.level() {
                Level::Error => {
                    style.set_color(Color::Red);
                }
                Level::Warn => {
                    style.set_color(Color::Yellow);
                }
                Level::Info => {
                    style.set_color(Color::Green);
                }
                Level::Debug | Level::Trace => {
                    style.set_color(Color::Blue);
                }
            }
            let indent_size = record.level().to_string().len() + 3;
            let indent = &" ".repeat(indent_size);
            let options = textwrap::Options::with_termwidth()
                .initial_indent(indent)
                .subsequent_indent(indent);
            writeln!(
                buf,
                "{} {}",
                style.value(format!("[{}]", record.level())),
                &textwrap::fill(&record.args().to_string(), &options)[indent_size..],
            )
        })
        .init();
}

// Parse the command-line options.
#[allow(clippy::too_many_lines)]
async fn settings() -> io::Result<Settings> {
    // Set up the command-line interface.
    let matches = App::new("Paxos")
        .version(VERSION)
        .author("Stephan Boyer <stephan@stephanboyer.com>")
        .about("This is an implementation of single-decree paxos.")
        .setting(AppSettings::ColoredHelp)
        .setting(AppSettings::NextLineHelp)
        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::VersionlessSubcommands)
        .arg(
            Arg::with_name(NODE_OPTION)
                .value_name("INDEX")
                .short("n")
                .long(NODE_OPTION)
                .help("Sets the index of the node corresponding to this instance")
                .required(true), // [tag:node_required]
        )
        .arg(
            Arg::with_name(PROPOSE_OPTION)
                .value_name("VALUE")
                .short("v")
                .long(PROPOSE_OPTION)
                .help("Proposes a value to the cluster"),
        )
        .arg(
            Arg::with_name(CONFIG_FILE_OPTION)
                .value_name("PATH")
                .short("c")
                .long(CONFIG_FILE_OPTION)
                .help(&format!(
                    "Sets the path of the config file (default: {})",
                    CONFIG_FILE_DEFAULT_PATH,
                )),
        )
        .arg(
            Arg::with_name(DATA_DIR_OPTION)
                .value_name("PATH")
                .short("d")
                .long(DATA_DIR_OPTION)
                .help(&format!(
                    "Sets the path of the directory in which to store persistent data \
                     (default: {})",
                    DATA_DIR_DEFAULT_PATH,
                )),
        )
        .arg(
            Arg::with_name(IP_OPTION)
                .value_name("ADDRESS")
                .short("i")
                .long(IP_OPTION)
                .help(
                    "Sets the IP address to run on \
                     (if different from the configuration)",
                ),
        )
        .arg(
            Arg::with_name(PORT_OPTION)
                .value_name("PORT")
                .short("p")
                .long(PORT_OPTION)
                .help("Sets the port to run on (if different from the configuration)"),
        )
        .get_matches();

    // Parse the config file path.
    let config_file_path = matches
        .value_of(CONFIG_FILE_OPTION)
        .unwrap_or(CONFIG_FILE_DEFAULT_PATH);

    // Parse the config file.
    let config = config::read(Path::new(config_file_path)).await?;

    // Parse the node index. The unwrap is safe due to [ref:node_required].
    let node_repr = matches.value_of(NODE_OPTION).unwrap();
    let node_index: usize = node_repr.parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "`{}` is not a valid node index. Reason: {}",
                node_repr,
                error,
            ),
        )
    })?;
    if node_index >= config.nodes.len() {
        // [tag:node_index_valid]
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("There is no node with index {}.", node_repr),
        ));
    }

    // Parse the IP address, if given.
    let ip = matches.value_of(IP_OPTION).map_or_else(
        || Ok(config.nodes[node_index].ip()), // [ref:node_index_valid]
        |raw_ip| {
            raw_ip.parse().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`{}` is not a valid IP address. Reason: {}", raw_ip, error),
                )
            })
        },
    )?;

    // Parse the port number, if given.
    let port = matches.value_of(PORT_OPTION).map_or_else(
        || Ok(config.nodes[node_index].port()), // [ref:node_index_valid]
        |raw_port| {
            raw_port.parse().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "`{}` is not a valid port number. Reason: {}",
                        raw_port,
                        error,
                    ),
                )
            })
        },
    )?;

    // Parse the data directory path.
    let data_dir_path = Path::new(
        matches
            .value_of(DATA_DIR_OPTION)
            .unwrap_or(DATA_DIR_DEFAULT_PATH),
    );

    // Determine the data file path [tag:data_file_path_has_parent].
    let data_file_path = Path::join(data_dir_path, format!("{}:{}", ip, port));

    // Return the settings.
    Ok(Settings {
        nodes: config.nodes,
        node_index,
        address: SocketAddr::new(ip, port),
        proposal: matches.value_of(PROPOSE_OPTION).map(ToString::to_string),
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
            error!("{}", error);
            exit(1);
        }
    };

    // Initialize the program state.
    let state = Arc::new(RwLock::new(initial()));

    // Attempt to read any persisted state.
    match state::read(&settings.data_file_path).await {
        Ok(persisted_state) => {
            let mut guard = state.write().await;
            *guard = persisted_state;
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

    // Run the acceptor and the proposer, if applicable.
    if let Err(error) = try_join!(
        acceptor::acceptor(state.clone(), &settings.data_file_path, settings.address),
        async {
            if let Some(value) = &settings.proposal {
                propose(
                    state,
                    &settings.data_file_path,
                    &settings.nodes,
                    settings.node_index,
                    value,
                )
                .await
            } else {
                Ok(())
            }
        },
    ) {
        error!("{}", error);
        exit(1);
    }
}
