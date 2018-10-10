#!/bin/bash

set -e
set -u

cargo web deploy --release -p client --target=wasm32-unknown-unknown
cargo run -p server
