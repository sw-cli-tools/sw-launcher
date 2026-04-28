#!/usr/bin/env bash
# scripts/serve.sh -- serve the sw-launcher static web assets
#
# Usage:
#   ./scripts/serve.sh
#
# Port 5264 is fixed; change here if the default conflicts.
#
# Prerequisite: basic-http-server (`cargo install basic-http-server`).
# Verify with: which basic-http-server

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WEB_DIR="${REPO_ROOT}/web"
PORT="${PORT:-5264}"
ADDR="127.0.0.1:${PORT}"

if [ ! -d "${WEB_DIR}" ]; then
    echo "error: ${WEB_DIR} does not exist" >&2
    exit 2
fi

if ! command -v basic-http-server >/dev/null 2>&1; then
    echo "error: basic-http-server not on PATH" >&2
    echo "       install with: cargo install basic-http-server" >&2
    exit 3
fi

echo "Serving ${WEB_DIR} at http://${ADDR}/"
echo "  memory layouts: http://${ADDR}/memory-layouts/"
echo "Press Ctrl-C to stop."
exec basic-http-server -a "${ADDR}" "${WEB_DIR}"
