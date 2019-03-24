mod config;

use clap::{App, Arg};
use futures::{future, Stream};
use hyper::{
  rt::Future, service::service_fn, Body, Method, Request, Response, Server,
  StatusCode,
};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::exit;

const CONFIG_FILE_PATH: &str = "config.yml";
const IP_OPTION: &str = "ip";
const NODE_OPTION: &str = "node";
const PORT_OPTION: &str = "port";

// This function handles incoming requests.
fn handler(
  req: Request<Body>,
) -> Box<Future<Item = Response<Body>, Error = hyper::Error> + Send> {
  if let (&Method::POST, "/echo") = (req.method(), req.uri().path()) {
    Box::new(req.into_body().concat2().map(|chunk| {
      let body = chunk.iter().cloned().collect::<Vec<u8>>();
      Response::new(Body::from(body))
    }))
  } else {
    let mut response = Response::new(Body::empty());
    *response.body_mut() = Body::from("Not found.");
    *response.status_mut() = StatusCode::NOT_FOUND;
    Box::new(future::ok(response))
  }
}

// Let the fun begin!
fn main() {
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
      Arg::with_name(IP_OPTION)
        .short("i")
        .long(IP_OPTION)
        .value_name("ADDRESS")
        .help(
          "Sets the IP address to run on (if different from the configured node)",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name(PORT_OPTION)
        .short("p")
        .long(PORT_OPTION)
        .value_name("PORT")
        .help(
          "Sets the port to run on (if different from the configured node)",
        )
        .takes_value(true),
    )
    .get_matches();

  // Parse the config file.
  let config_data =
    fs::read_to_string(CONFIG_FILE_PATH).unwrap_or_else(|_| {
      eprintln!("Error: Unable to read file `{}`.", CONFIG_FILE_PATH);
      exit(1);
    });
  let nodes_pre_me = config::parse(&config_data).unwrap_or_else(|err| {
    eprintln!(
      "Error: Unable to parse file `{}`. Reason: {}.",
      CONFIG_FILE_PATH, err
    );
    exit(1);
  });

  // Parse the node index.
  let node_repr = matches.value_of(NODE_OPTION).unwrap(); // [ref:node-required]
  let node_index: usize = node_repr.parse().unwrap_or_else(|_| {
    eprintln!("Error: `{}` is not a valid node index.", node_repr);
    exit(1);
  });
  if node_index >= nodes_pre_me.len() {
    eprintln!("Error: There is no node with index {}.", node_repr);
    exit(1); // [tag:node-index-valid]
  }
  let nodes: Vec<config::Node> = (0..nodes_pre_me.len())
    .map(|i| config::Node {
      me: i == node_index,
      ..nodes_pre_me[i]
    })
    .collect();

  // Parse the IP address, if given.
  let ip_repr: Option<&str> = matches.value_of(IP_OPTION);
  let ip: Ipv4Addr = ip_repr.map_or_else(
    || Ipv4Addr::UNSPECIFIED,
    |x| {
      x.parse().unwrap_or_else(|_| {
        eprintln!("Error: `{}` is not a valid IP address.", x);
        exit(1);
      })
    },
  );

  // Parse the port number, if given.
  let port_repr: Option<&str> = matches.value_of(PORT_OPTION);
  let port: u16 = port_repr.map_or_else(
    || nodes[node_index].address.port(), // [ref:node-index-valid]
    |x| {
      x.parse().unwrap_or_else(|_| {
        eprintln!("Error: `{}` is not a valid port number.", x);
        exit(1);
      })
    },
  );

  // Start the server.
  hyper::rt::run(hyper::rt::lazy(move || {
    let address = SocketAddr::V4(SocketAddrV4::new(ip, port));
    let server = Server::bind(&address)
      .serve(|| service_fn(handler))
      .map_err(|e| eprintln!("Server error: {}", e));
    println!("Listening on http://{}.", address);
    hyper::rt::spawn(server);
    Ok(())
  }));
}
