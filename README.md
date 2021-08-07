# Paxos

[![Build status](https://github.com/stepchowfun/paxos/workflows/Continuous%20integration/badge.svg?branch=main)](https://github.com/stepchowfun/paxos/actions?query=branch%3Amain)

An implementation of single-decree Paxos.

## Installation instructions

### Installation on macOS or Linux (x86-64)

If you're running macOS or Linux on an x86-64 CPU, you can install Paxos with this command:

```sh
curl https://raw.githubusercontent.com/stepchowfun/paxos/main/install.sh -LSfs | sh
```

The same command can be used again to update to the latest version.

The installation script supports the following optional environment variables:

- `VERSION=x.y.z` (defaults to the latest version)
- `PREFIX=/path/to/install` (defaults to `/usr/local/bin`)

For example, the following will install Paxos into the working directory:

```sh
curl https://raw.githubusercontent.com/stepchowfun/paxos/main/install.sh -LSfs | PREFIX=. sh
```

If you prefer not to use this installation method, you can download the binary from the [releases page](https://github.com/stepchowfun/paxos/releases), make it executable (e.g., with `chmod`), and place it in some directory in your [`PATH`](https://en.wikipedia.org/wiki/PATH_\(variable\)) (e.g., `/usr/local/bin`).

### Installation on Windows (x86-64)

If you're running Windows on an x86-64 CPU, download the latest binary from the [releases page](https://github.com/stepchowfun/paxos/releases) and rename it to `paxos` (or `paxos.exe` if you have file extensions visible). Create a directory called `Paxos` in your `%PROGRAMFILES%` directory (e.g., `C:\Program Files\Paxos`), and place the renamed binary in there. Then, in the "Advanced" tab of the "System Properties" section of Control Panel, click on "Environment Variables..." and add the full path to the new `Paxos` directory to the `PATH` variable under "System variables". Note that the `Program Files` directory might have a different name if Windows is configured for language other than English.

To update to an existing installation, simply replace the existing binary.

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
