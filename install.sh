#!/usr/bin/env sh

# This installer script supports Linux and macOS machines running on x86-64 only.

# Usage examples:
#   ./install.sh
#   VERSION=x.y.z ./install.sh
#   PREFIX=/usr/local/bin ./install.sh

# We wrap everything in parentheses to prevent the shell from executing only a prefix of the script
# if the download is interrupted.
(
  # Where the binary will be installed
  DESTINATION="${PREFIX:-/usr/local/bin}/paxos"

  # Which version to download
  RELEASE="v${VERSION:-1.0.0}"

  # Determine which binary to download.
  FILENAME=''
  if uname -a | grep -qi 'x86_64.*GNU/Linux'; then
    echo 'x86-64 GNU Linux detected.'
    FILENAME=paxos-x86_64-unknown-linux-gnu
  elif uname -a | grep -qi 'x86_64 Linux'; then
    echo 'x86-64 non-GNU Linux detected.'
    FILENAME=paxos-x86_64-unknown-linux-musl
  fi
  if uname -a | grep -qi 'Darwin.*x86_64'; then
    echo 'x86-64 macOS detected.'
    FILENAME=paxos-x86_64-apple-darwin
  fi

  # Find a temporary location for the binary.
  TEMPDIR=$(mktemp -d /tmp/paxos.XXXXXXXX)

  # This is a helper function to clean up and fail.
  fail() {
    echo "$1" >&2
    rm -rf "$TEMPDIR"
    exit 1
  }

  # Fail if there is no pre-built binary for this platform.
  if [ -z "$FILENAME" ]; then
    fail 'Unfortunately, there is no pre-built binary for this platform.'
  fi

  # Compute the full file path.
  SOURCE="$TEMPDIR/$FILENAME"

  # Download the binary.
  curl \
    "https://github.com/stepchowfun/paxos/releases/download/$RELEASE/$FILENAME" \
    -o "$SOURCE" -LSf || fail 'There was an error downloading the binary.'

  # Make it executable.
  chmod a+rx "$SOURCE" || fail 'There was an error setting the permissions for the binary.'

  # Install it at the requested destination.
  # shellcheck disable=SC2024
  mv -f "$SOURCE" "$DESTINATION" 2> /dev/null ||
    sudo mv -f "$SOURCE" "$DESTINATION" < /dev/tty ||
    fail "Unable to install the binary at $DESTINATION."

  # Remove the temporary directory.
  rm -rf "$TEMPDIR"

  # Let the user know it worked.
  echo "$(
    "$DESTINATION" --version || fail 'There was an error executing the binary.'
  ) is now installed."
)