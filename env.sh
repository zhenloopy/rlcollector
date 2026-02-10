#!/usr/bin/env bash
# Load development environment for RLCollector
# Usage: source env.sh

export PATH="$PATH:$USERPROFILE/.cargo/bin"

echo "RLCollector dev environment loaded (cargo: $(cargo --version 2>/dev/null || echo 'not found'))"
