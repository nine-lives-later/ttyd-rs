#!/bin/bash

set -e

DOCKER_BUILDKIT=1 docker build -t ttyd-rs .

docker run --rm -p 7681:7681 -it --name ttyd-rs ttyd-rs
