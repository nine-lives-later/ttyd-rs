#!/bin/bash
set -e

echo "Building frontend..."
pushd html > /dev/null
npm run build
popd > /dev/null

echo "Starting backend..."
killall ttyd-rs || true

cargo run -- "$@" --debug
