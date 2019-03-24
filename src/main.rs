mod config;
mod protocol;

use clap::{App, Arg};
use config::Node;
use futures::{future, stream::Stream, sync::mpsc};
use hyper::{
  rt, rt::Future, service::service_fn, Body, Client, Method, Request,
  Response, Server, StatusCode,
};
use protocol::{initial_state, State};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::exit;
use std::sync::{Arc, RwLock};

// We embed the favicon directly into the compiled binary.
const FAVICON_DATA: &[u8] = include_bytes!("../resources/favicon.ico");

// Defaults
const CONFIG_FILE_DEFAULT_PATH: &str = "config.yml";

// Command-line options
const CONFIG_OPTION: &str = "config";
const IP_OPTION: &str = "ip";
const NODE_OPTION: &str = "node";
const PORT_OPTION: &str = "port";
const PROPOSE_OPTION: &str = "propose";

struct Settings {
  nodes: Vec<Node>,
  ip: Ipv4Addr,
  port: u16,
  propose_value: Option<String>,
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
        .help(
          "Proposes a value to the cluster",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name(CONFIG_OPTION)
        .short("c")
        .long(CONFIG_OPTION)
        .value_name("PATH")
        .help(
          "Sets the path of the config file (default: config.yml)",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name(IP_OPTION)
        .short("i")
        .long(IP_OPTION)
        .value_name("ADDRESS")
        .help(
          "Sets the IP address to run on (if different from the configuration)",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name(PORT_OPTION)
        .short("p")
        .long(PORT_OPTION)
        .value_name("PORT")
        .help(
          "Sets the port to run on (if different from the configuration)",
        )
        .takes_value(true),
    )
    .get_matches();

  // Parse the config file path.
  let config_file_path = matches
    .value_of(CONFIG_OPTION)
    .unwrap_or(CONFIG_FILE_DEFAULT_PATH);

  // Parse the config file.
  let config_data =
    fs::read_to_string(config_file_path).unwrap_or_else(|err| {
      eprintln!(
        "Error: Unable to read file `{}`. Reason: {}",
        config_file_path, err
      );
      exit(1);
    });
  let nodes_pre_me = config::parse(&config_data).unwrap_or_else(|err| {
    eprintln!(
      "Error: Unable to parse file `{}`. Reason: {}.",
      config_file_path, err
    );
    exit(1);
  });

  // Parse the node index.
  let node_repr = matches.value_of(NODE_OPTION).unwrap(); // [ref:node-required]
  let node_index: usize = node_repr.parse().unwrap_or_else(|err| {
    eprintln!(
      "Error: `{}` is not a valid node index. Reason: {}",
      node_repr, err
    );
    exit(1);
  });
  if node_index >= nodes_pre_me.len() {
    eprintln!("Error: There is no node with index {}.", node_repr);
    exit(1); // [tag:node-index-valid]
  }
  let nodes: Vec<Node> = (0..nodes_pre_me.len())
    .map(|i| Node {
      me: i == node_index,
      ..nodes_pre_me[i]
    })
    .collect();

  // Parse the IP address, if given.
  let ip_repr: Option<&str> = matches.value_of(IP_OPTION);
  let ip: Ipv4Addr = ip_repr.map_or_else(
    || *nodes[node_index].address.ip(), // [ref:node-index-valid]
    |x| {
      x.parse().unwrap_or_else(|err| {
        eprintln!("Error: `{}` is not a valid IP address. Reason: {}", x, err);
        exit(1);
      })
    },
  );

  // Parse the port number, if given.
  let port_repr: Option<&str> = matches.value_of(PORT_OPTION);
  let port: u16 = port_repr.map_or_else(
    || nodes[node_index].address.port(), // [ref:node-index-valid]
    |x| {
      x.parse().unwrap_or_else(|err| {
        eprintln!(
          "Error: `{}` is not a valid port number. Reason: {}",
          x, err
        );
        exit(1);
      })
    },
  );

  // Return the settings.
  Settings {
    nodes,
    ip,
    port,
    propose_value: matches.value_of(PROPOSE_OPTION).map(|x| x.to_string()),
  }
}

// Run the program.
fn run(settings: Settings) {
  // Initialize the program state.
  let (quit_sender, quit_receiver) = mpsc::channel(0);
  let state = Arc::new(RwLock::new(initial_state(quit_sender)));

  // Set up the HTTP server.
  let address = SocketAddr::V4(SocketAddrV4::new(settings.ip, settings.port));
  let server = Server::try_bind(&address)
    .unwrap_or_else(|err| {
      eprintln!(
        "Error: Unable to bind to address `{}`. Reason: {}",
        address, err
      );
      exit(1);
    })
    .serve(move || {
      let state = state.clone();
      service_fn(
        move |req: Request<Body>| -> Box<
          Future<Item = Response<Body>, Error = hyper::Error> + Send,
        > {
          let state = state.clone();

          // This macro eliminates some boilerplate in the match expression
          // below. If Rust had higher-ranked types or let polymorphism, this
          // could have been implemented as a function.
          macro_rules! rpc {
            ( $x:ident ) => {
              Box::new(req.into_body().concat2().map(move |chunk| {
                let state = state.clone();
                let body = chunk.iter().cloned().collect::<Vec<u8>>();
                let payload = bincode::deserialize(&body).unwrap(); // Safe under non-Byzantine conditions
                let response = {
                  let mut state_borrow = state.write().unwrap(); // Safe since it can only fail if a panic already happened
                  protocol::$x(&payload, &mut state_borrow)
                };
                Response::new(Body::from(
                  bincode::serialize(&response).unwrap(),
                ))
              }))
            }
          }

          // Match on the route and handle the request appropriately.
          match (req.method(), req.uri().path()) {
            // RPC calls
            (&Method::POST, "/prepare") => rpc![prepare],
            (&Method::POST, "/accept") => rpc![accept],
            (&Method::POST, "/choose") => rpc![choose],

            // Health check
            (&Method::GET, "/health") => {
              // Respond with a representation of the program state.
              let state_repr = {
                let state_borrow: &State = &state.read().unwrap(); // Safe since it can only fail if a panic already happened
                serde_yaml::to_string(state_borrow).unwrap() // Safe since `State` has straightforward members
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
                  .unwrap(), // Safe since we constructed a well-formed response
              ))
            }

            // Catch-all
            _ => {
              // Respond with a generic 404 page.
              Box::new(future::ok(
                Response::builder()
                  .status(StatusCode::NOT_FOUND)
                  .body(Body::from("Not found."))
                  .unwrap(), // Safe since we constructed a well-formed response
              ))
            }
          }
        },
      )
    })
    .with_graceful_shutdown(
      quit_receiver
        .into_future()
        .map(|_| eprintln!("Shutting down...")),
    )
    .map_err(|e| eprintln!("Server error: {}", e));

  // Set up the HTTP client.
  let client = Client::new();

  // Start the runtime.
  rt::run(rt::lazy(move || {
    // Start the server.
    rt::spawn(server);

    // Propose a value if applicable.
    if let Some(value) = settings.propose_value {
      rt::spawn(protocol::propose(&client, &settings.nodes, &value));
    }

    // Tell the user that the server is running.
    println!("Listening on http://{}.", address);

    // Return control back to the runtime.
    Ok(())
  }));
}

// Let the fun begin!
fn main() {
  run(settings());
}
