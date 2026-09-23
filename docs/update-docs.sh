#!/bin/bash
set -euo pipefail

cargo doc --target-dir docs/ --document-private-items --all-features --output-format json -Z unstable-options
docs/update-indexes.sh
