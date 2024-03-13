#!/usr/bin/env bash
set -euxo pipefail

# Start the Paxos instances in the background.
echo 'Starting Paxos instance 0…'
LOG_LEVEL=debug "$PAXOS" --node 0 --propose foo | tee node-0.txt &
echo 'Starting Paxos instance 1…'
LOG_LEVEL=debug "$PAXOS" --node 1 --propose bar | tee node-1.txt &
echo 'Starting Paxos instance 2…'
LOG_LEVEL=debug "$PAXOS" --node 2 --propose baz | tee node-2.txt &

# Wait for the nodes to achieve consensus.
echo 'Waiting for Paxos instance 0…'
grep -q 'foo\|bar\|baz' <(tail -F node-0.txt)
echo 'Waiting for Paxos instance 1…'
grep -q 'foo\|bar\|baz' <(tail -F node-1.txt)
echo 'Waiting for Paxos instance 2…'
grep -q 'foo\|bar\|baz' <(tail -F node-2.txt)

# Kill all the subprocesses spawned by this script.
pkill -P "$$"

# Clean up the files.
rm node-0.txt node-1.txt node-2.txt
