mod acceptor;
mod config;
mod proposer;
mod protocol;
mod util;

#[macro_use]
extern crate log;

use clap::{App, Arg};
use env_logger::{Builder, Env};
use futures::prelude::*;
use hyper::{
  service::service_fn, Body, Client, Method, Request, Response, Server,
  StatusCode,
};
use proposer::propose;
use protocol::{initial_state, State};
use std::{
  fs,
  net::{Ipv4Addr, SocketAddr, SocketAddrV4},
  process::exit,
  sync::{Arc, RwLock},
  time::Duration,
};
use tokio::{prelude::*, timer::timeout};

// We embed the favicon directly into the compiled binary.
const FAVICON_DATA: &[u8] = include_bytes!("../resources/favicon.ico");

// The maximum amount of time the server will wait for the body of a request
const BODY_TIMEOUT: Duration = Duration::from_secs(1);

// Defaults
const CONFIG_FILE_DEFAULT_PATH: &str = "config.yml";

// Command-line option names
const CONFIG_OPTION: &str = "config";
const IP_OPTION: &str = "ip";
const NODE_OPTION: &str = "node";
const PORT_OPTION: &str = "port";
const PROPOSE_OPTION: &str = "propose";

// This struct represents a summary of the command-line options
struct Settings {
  nodes: Vec<SocketAddrV4>,
  node_index: usize,
  ip: Ipv4Addr,
  port: u16,
  proposal: Option<String>,
}

// Parse the command-line options.
fn settings() -> Settings {
  // Set up the command-line interface.
  let matches = App::new("Paxos")
    .version("1.0.0")
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
      Arg::with_name(CONFIG_OPTION)
        .short("c")
        .long(CONFIG_OPTION)
        .value_name("PATH")
        .help("Sets the path of the config file (default: config.yml)")
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
    .value_of(CONFIG_OPTION)
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

  // Return the settings.
  Settings {
    nodes: config.nodes,
    node_index,
    ip,
    port,
    proposal: matches.value_of(PROPOSE_OPTION).map(|x| x.to_string()),
  }
}

// Run the program.
fn run(settings: Settings) {
  // Initialize the program state.
  let state_for_acceptor = Arc::new(RwLock::new(initial_state()));
  let state_for_proposer = state_for_acceptor.clone();

  // Set up the HTTP server.
  let address = SocketAddr::V4(SocketAddrV4::new(settings.ip, settings.port));
  let server = Server::try_bind(&address)
    .unwrap_or_else(|e| {
      error!("Unable to bind to address `{}`. Reason: {}", address, e);
      exit(1);
    })
    .serve(move || {
      let state = state_for_acceptor.clone();
      service_fn(
        move |req: Request<Body>| -> Box<
          dyn Future<
              Item = Response<Body>,
              Error = timeout::Error<hyper::Error>,
            > + Send,
        > {
          let state = state.clone();

          // This macro eliminates some boilerplate in the match expression
          // below. If Rust had higher-ranked types or let polymorphism, this
          // could have been implemented as a function.
          macro_rules! rpc {
            ($x:ident) => {
              Box::new(req.into_body().concat2().timeout(BODY_TIMEOUT).map(
                move |chunk| {
                  let state = state.clone();
                  let body = chunk.iter().cloned().collect::<Vec<u8>>();
                  // The `unwrap` is safe under non-Byzantine conditions
                  let payload = bincode::deserialize(&body).unwrap();
                  let response = {
                    // The `unwrap` is safe since it can only fail if a panic
                    // already happened.
                    let mut state_borrow = state.write().unwrap();
                    acceptor::$x(&payload, &mut state_borrow)
                  };
                  Response::new(Body::from(
                    bincode::serialize(&response).unwrap(),
                  ))
                },
              ))
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
              Box::new(future::ok(Response::new(Body::from(format!(
                "System operational.\n\n{}",
                state_repr
              )))))
            }

            // Favicon
            (&Method::GET, "/favicon.ico") => {
              // Respond with the favicon.
              Box::new(future::ok(
                Response::builder()
                  .header(hyper::header::CONTENT_TYPE, "image/x-icon")
                  .body(Body::from(FAVICON_DATA))
                  // The `unwrap` is safe since we constructed a well-formed
                  // response.
                  .unwrap(),
              ))
            }

            // Catch-all
            _ => {
              // Respond with a generic 404 page.
              Box::new(future::ok(
                Response::builder()
                  .status(StatusCode::NOT_FOUND)
                  .body(Body::from("Not found."))
                  // The `unwrap` is safe since we constructed a well-formed
                  // response.
                  .unwrap(),
              ))
            }
          }
        },
      )
    })
    .map_err(|e| error!("Server error: {}", e));

  // Set up the HTTP client.
  let client = Client::new();

  // Start the runtime.
  tokio::run(future::lazy(move || {
    // Start the server.
    tokio::spawn(server);

    // Propose a value if applicable.
    if let Some(value) = settings.proposal {
      tokio::spawn(propose(
        &client,
        &settings.nodes,
        settings.node_index,
        &value,
        state_for_proposer,
      ));
    }

    // Tell the user that the server is running.
    info!("Listening on http://{}.", address);

    // Return control back to the runtime.
    Ok(())
  }));
}

// Let the fun begin!
fn main() {
  // Set up the logger.
  Builder::from_env(
    Env::default().filter("LOG_LEVEL").write_style("LOG_STYLE"),
  )
  .format(|buf, record| {
    writeln!(buf, "[{}] {}", record.level(), record.args())
  })
  .init();

  // Run Paxos!
  run(settings());
}
