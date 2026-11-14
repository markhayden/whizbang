#!/usr/bin/env bash
# Writes a tool record's generated parts again (SPEC.md section 8, "Tool records"): the
# example images beside the record, its measurements, its times and its comparison of
# methods. Rerun it when the tool changes, and commit what it writes.
#
#   scripts/tool-record.sh view      docs/reference/tools/view.md and view/
#   scripts/tool-record.sh grow      docs/reference/tools/grow.md and grow/
#
# It builds in release mode and times real work, so it is heavy: run it through
# scripts/heavy.sh when another agent may be building. The fast tests then hold the record
# to the tool: the images byte for byte, and the measurements that do not depend on the
# machine. The script keeps the times the record holds unless one of them moved by more
# than a third, or something else between the marks changed. That assumes a quiet
# machine: when a run on a busy one changes the times alone, discard the change and run
# this again. The test that writes the record runs through scripts/thread-guard.sh, as
# every test run does.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

case "${1:-}" in
  view)
    scripts/thread-guard.sh cargo test --release -p whizbang-squint --test it -- \
      --ignored --exact record::write_the_record
    ;;
  grow)
    scripts/thread-guard.sh cargo test --release -p whizbang-rascal --test it -- \
      --ignored --exact record::write_the_record
    ;;
  *)
    echo "tool-record: usage: scripts/tool-record.sh view|grow" >&2
    exit 2
    ;;
esac
echo "tool-record: wrote docs/reference/tools/$1.md and docs/reference/tools/$1/"
}
main "$@"; exit
