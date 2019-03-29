# Paxos

[![Build Status](https://travis-ci.org/stepchowfun/paxos.svg?branch=master)](https://travis-ci.org/stepchowfun/paxos)

An implementation of single-decree Paxos.

## Installation

You can build and install the program with [Cargo](https://doc.rust-lang.org/book/second-edition/ch14-04-installing-binaries.html):

```sh
cargo install --force --path .
```

You can run that command again to update an existing installation.

## Configuration

By default, the program looks for a configuration file named `config.yml` in the working directory. This file describes the cluster membership. An example configuration is provided in this repository.

## Usage

For a simple demonstration, run the following commands in separate terminals:

```sh
paxos --node 0 --propose foo
paxos --node 1 --propose bar
paxos --node 2 --propose baz
```

Here are the full usage instructions:

```
USAGE:
    paxos [OPTIONS] --node <INDEX>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --config <PATH>      Sets the path of the config file (default:
                             config.yml)
    -i, --ip <ADDRESS>       Sets the IP address to run on (if different from
                             the configuration)
    -n, --node <INDEX>       Sets the index of the node corresponding to this
                             instance
    -p, --port <PORT>        Sets the port to run on (if different from the
                             configuration)
    -v, --propose <VALUE>    Proposes a value to the cluster
```

## References

The Paxos algorithm was first described in [1].

1. Leslie Lamport. 1998. The part-time parliament. ACM Trans. Comput. Syst. 16, 2 (May 1998), 133-169. DOI: https://doi.org/10.1145/279227.279229
