# Paxos

[![Build status](https://github.com/stepchowfun/paxos/workflows/Continuous%20integration/badge.svg?branch=main)](https://github.com/stepchowfun/paxos/actions?query=branch%3Amain)

An implementation of single-decree Paxos.

## Installation instructions

### Easy installation on macOS or Linux

If you are running macOS or Linux on an x86-64 CPU, you can install Paxos with this command:

```sh
curl https://raw.githubusercontent.com/stepchowfun/paxos/main/install.sh -LSfs | sh
```

The same command can be used again to update Paxos to the latest version.

**NOTE:** Piping `curl` to `sh` is considered dangerous by some since the server might be compromised. If you're concerned about this, you can download and inspect the installation script or choose one of the other installation methods.

#### Customizing the installation

The installation script supports the following environment variables:

- `VERSION=x.y.z` (defaults to the latest version)
- `PREFIX=/path/to/install` (defaults to `/usr/local/bin`)

For example, the following will install Paxos into the working directory:

```sh
curl https://raw.githubusercontent.com/stepchowfun/paxos/main/install.sh -LSfs | PREFIX=. sh
```

### Manual installation for macOS, Linux, or Windows

The [releases page](https://github.com/stepchowfun/paxos/releases) has precompiled binaries for macOS, Linux, and Windows systems running on an x86-64 CPU. You can download one of them and place it in a directory listed in your [`PATH`](https://en.wikipedia.org/wiki/PATH_\(variable\)).

### Installation with Cargo

If you have [Cargo](https://doc.rust-lang.org/cargo/), you can install Paxos as follows:

```sh
cargo install paxos
```

You can run that command with `--force` to update an existing installation.

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
    paxos --node <INDEX>

OPTIONS:
    -c, --config-file <PATH>
            Sets the path of the config file (default: config.yml)

    -d, --data-dir <PATH>
            Sets the path of the directory in which to store persistent data (default: data)

    -h, --help
            Prints help information

    -i, --ip <ADDRESS>
            Sets the IP address to run on (if different from the configuration)

    -n, --node <INDEX>
            Sets the index of the node corresponding to this instance

    -p, --port <PORT>
            Sets the port to run on (if different from the configuration)

    -v, --propose <VALUE>
            Proposes a value to the cluster

    -V, --version
            Prints version information
```

## References

The Paxos algorithm was first described in [1].

1. Leslie Lamport. 1998. The part-time parliament. ACM Trans. Comput. Syst. 16, 2 (May 1998), 133-169. DOI: https://doi.org/10.1145/279227.279229
