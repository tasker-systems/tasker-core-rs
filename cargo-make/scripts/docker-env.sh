#!/usr/bin/env bash
# TAS-78: Container Runtime Detection
#
# Detects whether to use docker or podman and sets DOCKER_CMD and COMPOSE_CMD.
# Source this file in scripts that need container commands.
#
# Usage:
#   source "${SCRIPTS_DIR}/docker-env.sh"
#   $DOCKER_CMD ps
#   $COMPOSE_CMD up -d

# Detect container runtime
if command -v podman &> /dev/null; then
    export DOCKER_CMD="podman"
    export COMPOSE_CMD="podman compose"
elif command -v docker &> /dev/null; then
    export DOCKER_CMD="docker"
    # Check if docker compose (v2) is available, else fall back to docker-compose
    if docker compose version &> /dev/null 2>&1; then
        export COMPOSE_CMD="docker compose"
    else
        export COMPOSE_CMD="docker-compose"
    fi
else
    echo "❌ Neither docker nor podman found in PATH" >&2
    exit 1
fi
