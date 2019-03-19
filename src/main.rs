use clap::{App, Arg};
use futures::{future, Stream};
use hyper::{
  rt::Future, service::service_fn, Body, Method, Request, Response, Server,
  StatusCode,
};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::exit;

const PORT_DEFAULT: &str = "3000";
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
      Arg::with_name(PORT_OPTION)
        .short("p")
        .long(PORT_OPTION)
        .value_name("PORT")
        .help(
          &format!("Sets the port to run on (default: {})", PORT_DEFAULT)
            .to_owned(),
        )
        .takes_value(true),
    )
    .get_matches();

  // Parse the port number.
  let port_repr = matches.value_of(PORT_OPTION).unwrap_or(PORT_DEFAULT);
  let port = port_repr.parse().unwrap_or_else(|_| {
    eprintln!("Error: `{}` is not a valid port number.", port_repr);
    exit(1);
  });

  // Start the server.
  hyper::rt::run(
    Server::bind(&SocketAddr::V4(SocketAddrV4::new(
      Ipv4Addr::UNSPECIFIED,
      port,
    )))
    .serve(|| service_fn(handler))
    .map_err(|e| eprintln!("Server error: {}", e)),
  );
}
