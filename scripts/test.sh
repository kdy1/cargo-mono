#!/usr/bin/env bash
set -eu

cargo install --locked --path . --debug

# Run cargo bump
(cd $1 && cargo mono bump -i)