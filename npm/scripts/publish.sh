#!/usr/bin/env bash
# Publish the entire npm distribution: every cli-* platform package
# first (so the main package's optionalDependencies resolve), then the
# main `umadev` package last.
#
# Assumes:
#   - `stage.sh` has already populated each `npm/cli-<platform>/bin/`
#     with the matching prebuilt binary.
#   - `npm whoami` is logged in with publish rights to the `@umacloud`
#     scope and to the `umadev` name.
#   - All package.json versions are aligned (this script does NOT bump).
#
# Use `--dry-run` to validate without actually publishing.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NPM_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN="--dry-run"
  echo "▶ publish.sh: DRY RUN (nothing will actually publish)"
fi

PLATFORM_PACKAGES=(
  "cli-darwin-arm64"
  "cli-darwin-x64"
  "cli-linux-x64"
  "cli-linux-arm64"
  "cli-win32-x64"
)

# Refuse a split release before the first irreversible publish. The main
# package, every platform package, the knowledge package, Cargo, and each exact
# optional-dependency pin must name the same version.
CARGO_VERSION="$(sed -n 's/^version = "\([0-9][0-9.]*\)"/\1/p' "$NPM_ROOT/../Cargo.toml" | head -1)"
node - "$NPM_ROOT" "$CARGO_VERSION" <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const [root, expected] = process.argv.slice(2);
const dirs = fs.readdirSync(root, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && fs.existsSync(path.join(root, entry.name, 'package.json')))
  .map((entry) => entry.name);
for (const dir of dirs) {
  const pkg = JSON.parse(fs.readFileSync(path.join(root, dir, 'package.json'), 'utf8'));
  if (pkg.version !== expected) throw new Error(`${pkg.name}: ${pkg.version} != Cargo ${expected}`);
  if (pkg.name === 'umadev') {
    for (const [name, version] of Object.entries(pkg.optionalDependencies || {})) {
      if (version !== expected) throw new Error(`${name} pin ${version} != ${expected}`);
    }
  }
}
console.log(`publish.sh: version lockstep verified (${expected})`);
NODE

# 1) Verify every platform package has its binary staged.
for pkg in "${PLATFORM_PACKAGES[@]}"; do
  case "$pkg" in
    cli-win32-*) bin="umadev.exe" ;;
    *)           bin="umadev" ;;
  esac
  if [[ ! -f "$NPM_ROOT/$pkg/bin/$bin" ]]; then
    echo "publish.sh: missing $NPM_ROOT/$pkg/bin/$bin" >&2
    echo "             run stage.sh for this platform first" >&2
    exit 1
  fi
done

# 2) Publish each platform package (scoped, public access).
for pkg in "${PLATFORM_PACKAGES[@]}"; do
  echo "▶ publish.sh: npm publish $pkg..."
  (cd "$NPM_ROOT/$pkg" && npm publish --access public $DRY_RUN)
done

# 2b) The embedding model is NO LONGER shipped on npm. The ~224MB fp16 model
#     exceeds npm's package size limit, so the CLI shim fetches it on first
#     run into ~/.umadev/embed-model (see npm/umadev/bin/cli.js). RAG is local
#     and fully functional without it (BM25), so nothing to publish here.

# 2c) Publish the knowledge corpus package (main depends on it). Stage the
#     repo's knowledge/ tree into it first (CI / ephemeral).
echo "publish.sh: staging + npm publish @umacloud/knowledge..."
if [[ -d "$NPM_ROOT/../knowledge" ]]; then
  cp -R "$NPM_ROOT/../knowledge/." "$NPM_ROOT/knowledge-corpus/"
  (cd "$NPM_ROOT/knowledge-corpus" && npm publish --access public $DRY_RUN)
else
  echo "publish.sh: skipping @umacloud/knowledge (knowledge/ not found)" >&2
fi

# 3) Publish the main package last (so its optionalDependencies resolve
#    against versions that already exist on the registry).
echo "▶ publish.sh: npm publish umadev (main)..."
(cd "$NPM_ROOT/umadev" && npm publish --access public $DRY_RUN)

echo "✓ publish.sh: done"
