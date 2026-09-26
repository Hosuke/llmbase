#!/usr/bin/env bash
set -euo pipefail
# Run in the implementation worktree; consumes the existing package-lock exactly.
npm --prefix frontend ci --no-audit --no-fund
node frontend/node_modules/typescript/bin/tsc --noEmit --project frontend/tsconfig.app.json
npm --prefix frontend run build
