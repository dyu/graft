#!/bin/bash
set -euo pipefail

fly deploy --config deploy/metastore/fly.toml
fly deploy --config deploy/pagestore/fly.toml
