extern crate clap;

fn main() {
  // Set up the command-line interface.
  clap::App::new("Paxos")
    .version("1.0.0")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("This is an implementation of single-decree paxos.")
    .get_matches();
}
