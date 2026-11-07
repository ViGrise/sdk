#!/usr/bin/env bash
# Dependency advisory gate.
#
#   scripts/advisories.sh [--open-issues]
#
# Scans the lock files with Trivy. Each advisory must have an open issue with the
# label `dependency-advisory` and a title that starts with the advisory ID. A
# tracked advisory prints a warning, and an untracked one fails the check. With
# --open-issues, the script opens an issue for each untracked advisory. Requires
# trivy, jq and an authenticated gh. The Rust workspace commits no lock file, so
# the caller generates rust/Cargo.lock first.

set -euo pipefail

cd "$(dirname "$0")/.."

LABEL="dependency-advisory"
OPEN_ISSUES=0
for arg in "$@"; do
  case "$arg" in
    --open-issues) OPEN_ISSUES=1 ;;
    *) echo "usage: scripts/advisories.sh [--open-issues]" >&2; exit 2 ;;
  esac
done

if [[ ! -f rust/Cargo.lock ]]; then
  echo "::error::rust/Cargo.lock is missing, so the Rust dependencies would go unscanned" >&2
  exit 1
fi

report="$(mktemp)"
trap 'rm -f "$report"' EXIT
trivy fs --quiet --scanners vuln --format json --output "$report" .

# Listing reads issues directly. The search index can miss a new issue.
tracked="$(gh issue list --state open --label "$LABEL" --limit 500 --json title --jq '.[].title')"

failed=0
while IFS= read -r finding; do
  id="$(jq -r '.id' <<<"$finding")"
  if awk -F: -v id="$id" '$1 == id { found = 1 } END { exit !found }' <<<"$tracked"; then
    echo "::warning::$id is tracked in an open issue"
    continue
  fi
  failed=1
  if [[ "$OPEN_ISSUES" == 0 ]]; then
    echo "::error::$id has no open issue with the label $LABEL"
    continue
  fi
  body="$(jq -r '
    "> \(.title)\n\n| Details | |\n| --- | --- |\n"
    + "| Severity | \(.severity) |\n"
    + "| URL | \(.url) |\n\n"
    + "| Lock file | Package | Installed | Fixed in |\n| --- | --- | --- | --- |\n"
    + (.rows | map("| `\(.target)` | `\(.package)` | \(.installed) | \(.fixed) |") | join("\n"))
    + "\n\nClose this issue when the lock files no longer contain an affected version."
    ' <<<"$finding")"
  gh label create "$LABEL" --color B60205 \
    --description "A published advisory that affects the dependency tree" >/dev/null 2>&1 || true
  issue="$(gh issue create --label "$LABEL" --title "$id: $(jq -r '.title' <<<"$finding")" --body "$body")"
  echo "::error::$id is new. Opened $issue"
done < <(jq -c '
  [.Results[]? | .Target as $target | .Vulnerabilities[]? | . + {Target: $target}]
  | group_by(.VulnerabilityID)[]
  | {
      id: .[0].VulnerabilityID,
      title: (.[0].Title // .[0].PkgName),
      severity: .[0].Severity,
      url: (.[0].PrimaryURL // "-"),
      rows: (map({target: .Target, package: .PkgName, installed: .InstalledVersion, fixed: (.FixedVersion // "-")}) | unique)
    }' "$report")

exit "$failed"
