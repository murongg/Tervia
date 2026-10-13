#!/bin/zsh
set -euo pipefail
TERVIA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$TERVIA_ROOT/scripts/package.sh"
open "$TERVIA_ROOT/build/Tervia.app"
