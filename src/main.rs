mod acceptor;
mod config;
mod proposer;
mod state;
mod util;

#[macro_use]
extern crate log;

use clap::{App, Arg};
use env_logger::{fmt::Color, Builder};
use futures::{future::ok, prelude::*};
use hyper::{
  header::CONTENT_TYPE, service::service_fn, Body, Client, Method, Request,
  Response, Server, StatusCode,
};
use log::{Level, LevelFilter};
use proposer::propose;
use state::{initial, State};
use std::{
  env,
  error::Error,
  fs,
  net::{Ipv4Addr, SocketAddr, SocketAddrV4},
  path::{Path, PathBuf},
  process::exit,
  str::FromStr,
  string::ToString,
  sync::{Arc, RwLock},
  time::Duration,
};
use textwrap::Wrapper;
use tokio::prelude::*;

// The program version
const VERSION: &str = env!("CARGO_PKG_VERSION");

// We embed the favicon directly into the compiled binary.
const FAVICON_DATA: &[u8] = include_bytes!("../resources/favicon.ico");

// The maximum amount of time the server will wait for the body of a request
const BODY_TIMEOUT: Duration = Duration::from_secs(1);

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
  nodes: Vec<SocketAddrV4>,
  node_index: usize,
  ip: Ipv4Addr,
  port: u16,
  proposal: Option<String>,
  data_file_path: PathBuf,
}

// Parse the command-line options.
fn settings() -> Settings {
  // Set up the command-line interface.
  let matches = App::new("Paxos")
    .version(VERSION)
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("This is an implementation of single-decree paxos.")
    .arg(
      Arg::with_name(NODE_OPTION)
        .short("n")
        .long(NODE_OPTION)
        .value_name("INDEX")
        .help("Sets the index of the node corresponding to this instance")
        .takes_value(true)
        .required(true), // [tag:node-required]
    )
    .arg(
      Arg::with_name(PROPOSE_OPTION)
        .short("v")
        .long(PROPOSE_OPTION)
        .value_name("VALUE")
        .help("Proposes a value to the cluster")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(CONFIG_FILE_OPTION)
        .short("c")
        .long(CONFIG_FILE_OPTION)
        .value_name("PATH")
        .help(&format!(
          "Sets the path of the config file (default: {})",
          CONFIG_FILE_DEFAULT_PATH,
        ))
        .takes_value(true),
    )
    .arg(
      Arg::with_name(DATA_DIR_OPTION)
        .short("d")
        .long(DATA_DIR_OPTION)
        .value_name("PATH")
        .help(&format!(
          "Sets the path of the directory in which to store persistent data \
           (default: {})",
          DATA_DIR_DEFAULT_PATH,
        ))
        .takes_value(true),
    )
    .arg(
      Arg::with_name(IP_OPTION)
        .short("i")
        .long(IP_OPTION)
        .value_name("ADDRESS")
        .help(
          "Sets the IP address to run on \
           (if different from the configuration)",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name(PORT_OPTION)
        .short("p")
        .long(PORT_OPTION)
        .value_name("PORT")
        .help("Sets the port to run on (if different from the configuration)")
        .takes_value(true),
    )
    .get_matches();

  // Parse the config file path.
  let config_file_path = matches
    .value_of(CONFIG_FILE_OPTION)
    .unwrap_or(CONFIG_FILE_DEFAULT_PATH);

  // Parse the config file.
  let config_data = fs::read_to_string(config_file_path).unwrap_or_else(|e| {
    error!("Unable to read file `{}`. Reason: {}", config_file_path, e);
    exit(1);
  });
  let config = config::parse(&config_data).unwrap_or_else(|e| {
    error!(
      "Unable to parse file `{}`. Reason: {}.",
      config_file_path, e
    );
    exit(1);
  });

  // Parse the node index.
  // The unwrap is safe due to [ref:node-required].
  let node_repr = matches.value_of(NODE_OPTION).unwrap();
  let node_index: usize = node_repr.parse().unwrap_or_else(|e| {
    error!("`{}` is not a valid node index. Reason: {}", node_repr, e);
    exit(1);
  });
  if node_index >= config.nodes.len() {
    error!("There is no node with index {}.", node_repr);
    exit(1); // [tag:node-index-valid]
  }

  // Parse the IP address, if given.
  let ip = matches.value_of(IP_OPTION).map_or_else(
    || *config.nodes[node_index].ip(), // [ref:node-index-valid]
    |x| {
      x.parse().unwrap_or_else(|e| {
        error!("`{}` is not a valid IP address. Reason: {}", x, e);
        exit(1);
      })
    },
  );

  // Parse the port number, if given.
  let port = matches.value_of(PORT_OPTION).map_or_else(
    || config.nodes[node_index].port(), // [ref:node-index-valid]
    |x| {
      x.parse().unwrap_or_else(|e| {
        error!("`{}` is not a valid port number. Reason: {}", x, e);
        exit(1);
      })
    },
  );

  // Parse the data directory path.
  let data_dir_path = Path::new(
    matches
      .value_of(DATA_DIR_OPTION)
      .unwrap_or(DATA_DIR_DEFAULT_PATH),
  );

  // Determine the data file path [tag:data-file-path-has-parent].
  let data_file_path = Path::join(data_dir_path, format!("{}:{}", ip, port));

  // Return the settings.
  Settings {
    nodes: config.nodes,
    node_index,
    ip,
    port,
    proposal: matches.value_of(PROPOSE_OPTION).map(ToString::to_string),
    data_file_path,
  }
}

// Run the program.
fn run(settings: Settings) -> impl Future<Item = (), Error = ()> {
  // Initialize the program state.
  let state = Arc::new(RwLock::new(initial()));

  // Attempt to read the persisted state.
  state::read(state.clone(), &settings.data_file_path).then(
    move |read_result| {
      // Inform the user whether the read succeeded.
      if read_result.is_ok() {
        info!("State loaded from persistent storage.");
      } else {
        info!("Starting from the initial state.");
      }

      // Clone some data that will outlive this function.
      let state_for_acceptor = state.clone();
      let state_for_proposer = state_for_acceptor.clone();
      let settings_for_acceptor = settings.clone();

      // Set up the HTTP server.
      let address =
        SocketAddr::V4(SocketAddrV4::new(settings.ip, settings.port));
      let server = Server::try_bind(&address)
        .unwrap_or_else(|e| {
          error!("Unable to bind to address `{}`. Reason: {}", address, e);
          exit(1);
        })
        .serve(move || {
          let state = state_for_acceptor.clone();
          let settings = settings_for_acceptor.clone();
          service_fn(
            move |req: Request<Body>| -> Box<
              dyn Future<
                  Item = Response<Body>,
                  Error = Box<dyn Error + Send + Sync>,
                > + Send,
            > {
              let state_for_request = state.clone();
              let state_for_write = state.clone();
              let settings = settings.clone();

              // This macro eliminates some boilerplate in the match expression
              // below. If Rust had higher-ranked types or let polymorphism,
              // this could have been implemented as a function.
              macro_rules! rpc {
                ($x:ident) => {
                  Box::new(
                    req.into_body().concat2().timeout(BODY_TIMEOUT).map(
                      move |chunk| {
                        let state = state_for_request.clone();
                        let body = chunk.iter().cloned().collect::<Vec<u8>>();

                        // The `unwrap` is safe under non-Byzantine conditions
                        let payload = bincode::deserialize(&body).unwrap();

                        // The `unwrap` is safe since it can only fail if a
                        // panic already happened.
                        let mut state_borrow = state.write().unwrap();
                        acceptor::$x(&payload, &mut state_borrow)
                      }
                    ).map_err(|e|
                      Box::new(e) as Box<dyn Error + Send + Sync>
                    ).and_then(move |response| {
                      let state = state_for_write.clone();
                      let settings = settings.clone();

                      // The `unwrap` is safe since it can only fail if a panic
                      // already happened.
                      let state_borrow = state.read().unwrap();

                      state::write(&state_borrow, &settings.data_file_path)
                        .map(|_| response)
                        .map_err(|e|
                          Box::new(e) as Box<dyn Error + Send + Sync>
                        )
                    }).map(|response|
                      Response::new(Body::from(
                        // The `unwrap` is safe because serialization should
                        // never fail.
                        bincode::serialize(&response).unwrap(),
                      ))
                    )
                  )
                };
              }

              // Match on the route and handle the request appropriately.
              match (req.method(), req.uri().path()) {
                // RPC calls
                (&Method::POST, acceptor::PREPARE_ENDPOINT) => rpc![prepare],
                (&Method::POST, acceptor::ACCEPT_ENDPOINT) => rpc![accept],
                (&Method::POST, acceptor::CHOOSE_ENDPOINT) => rpc![choose],

                // Summary of the program state
                (&Method::GET, "/") => {
                  // Respond with a representation of the program state.
                  let state_repr = {
                    // The `unwrap` is safe since it can only fail if a panic
                    // already happened.

                    let state_borrow: &State = &state.read().unwrap();
                    // The `unwrap` is safe because serialization should never
                    // fail.
                    serde_yaml::to_string(state_borrow).unwrap()
                  };
                  Box::new(ok(Response::new(Body::from(format!(
                    "System operational.\n\n{}",
                    state_repr
                  )))))
                }

                // Favicon
                (&Method::GET, "/favicon.ico") => {
                  // Respond with the favicon.
                  Box::new(ok(
                    Response::builder()
                      .header(CONTENT_TYPE, "image/x-icon")
                      .body(Body::from(FAVICON_DATA))
                      // The `unwrap` is safe since we constructed a
                      // well-formed response.
                      .unwrap(),
                  ))
                }

                // Catch-all
                _ => {
                  // Respond with a generic 404 page.
                  Box::new(ok(
                    Response::builder()
                      .status(StatusCode::NOT_FOUND)
                      .body(Body::from("Not found."))
                      // The `unwrap` is safe since we constructed a
                      // well-formed response.
                      .unwrap(),
                  ))
                }
              }
            },
          )
        })
        .map_err(|e| error!("Server error: {}", e));

      // Propose a value if applicable.
      let client = if let Some(value) = settings.proposal {
        Box::new(propose(
          &Client::new(),
          &settings.nodes,
          settings.node_index,
          state_for_proposer,
          &settings.data_file_path,
          &value,
        )) as Box<Future<Item = (), Error = ()> + Send>
      } else {
        Box::new(ok(())) as Box<Future<Item = (), Error = ()> + Send>
      };

      // Tell the user the address of the server.
      info!("Listening on http://{}", address);

      // Run the server and the client.
      server.join(client).map(|_| ())
    },
  )
}

// Let the fun begin!
fn main() {
  // Set up the logger.
  Builder::new()
    .filter_module(
      module_path!(),
      LevelFilter::from_str(
        &env::var("LOG_LEVEL")
          .unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string()),
      )
      .unwrap_or_else(|_| DEFAULT_LOG_LEVEL),
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
      writeln!(
        buf,
        "{} {}",
        style.value(format!("[{}]", record.level())),
        &Wrapper::with_termwidth()
          .initial_indent(indent)
          .subsequent_indent(indent)
          .fill(&record.args().to_string())[indent_size..]
      )
    })
    .init();

  // Run the program!
  tokio::run(run(settings()));
}
