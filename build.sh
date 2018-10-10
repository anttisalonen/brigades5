#!/bin/bash

set -e
set -u

cargo web build --release -p client --target=wasm32-unknown-unknown
cargo build -p server
