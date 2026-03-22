#!/usr/bin/env bash
# Bump the version across all packages simultaneously.
#
# Usage: ./scripts/bump-version.sh 0.2.0
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>" >&2
  echo "Example: $0 0.2.0" >&2
  exit 1
fi

VERSION="$1"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Bumping to v${VERSION}…"

# 1. Cargo.toml
sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" "$ROOT/Cargo.toml"
echo "  ✓ Cargo.toml"

# 2. npm/package.json
sed -i "s/\"version\": \".*\"/\"version\": \"${VERSION}\"/" "$ROOT/npm/package.json"
echo "  ✓ npm/package.json"

# 3. packages/python/pyproject.toml
sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" "$ROOT/packages/python/pyproject.toml"
echo "  ✓ packages/python/pyproject.toml"

# 4. packages/python/prova_pdf/__init__.py
sed -i "s/^__version__ = \".*\"/__version__ = \"${VERSION}\"/" "$ROOT/packages/python/prova_pdf/__init__.py"
echo "  ✓ packages/python/prova_pdf/__init__.py"

echo ""
echo "Done. Next steps:"
echo "  git add -u && git commit -m 'chore: bump version to ${VERSION}'"
echo "  git tag v${VERSION}"
echo "  git push origin master v${VERSION}"
