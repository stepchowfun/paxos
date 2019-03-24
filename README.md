# Paxos

[![Build Status](https://travis-ci.org/stepchowfun/paxos.svg?branch=master)](https://travis-ci.org/stepchowfun/paxos)

An implementation of single-decree Paxos.

## Installation

You can build and install with [Cargo](https://doc.rust-lang.org/book/second-edition/ch14-04-installing-binaries.html):

```sh
cargo install --force --path .
```

You can run that command again to update an existing installation.

## Usage

```
USAGE:
    paxos [OPTIONS] --node <INDEX>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -n, --node <INDEX>    Sets the index of the node corresponding to this
                          instance
    -p, --port <PORT>     Sets the port to run on (if different from the
                          configured node)
```

## References

The Paxos algorithm was first described in [1].

1. Leslie Lamport. 1998. The part-time parliament. ACM Trans. Comput. Syst. 16, 2 (May 1998), 133-169. DOI: https://doi.org/10.1145/279227.279229
