#!/usr/bin/env bash
# Writes a tool record's generated parts again (SPEC.md section 8, "Tool records"): the
# example images beside the record, its measurements, its times and its comparison of
# methods. Rerun it when the tool changes, and commit what it writes.
#
#   scripts/tool-record.sh view      docs/reference/tools/view.md and view/
#   scripts/tool-record.sh grow      docs/reference/tools/grow.md and grow/
#   scripts/tool-record.sh align     docs/reference/tools/align.md and align/
#   scripts/tool-record.sh resize    docs/reference/tools/resize.md and resize/
#   scripts/tool-record.sh tiles     the example images of docs/reference/tools/tiles.md, in tiles/
#   scripts/tool-record.sh change_map, or edge_similarity: both records, change_map.md and
#                                    edge_similarity.md, and their folders
#   scripts/tool-record.sh close, subtract, line, rect, stroke, paste, fill, mask_rect or
#                                    protect: that record and its folder (plan m3, task D1;
#                                    crates/whizbang-rascal/tests/it/tool_records/)
#   scripts/tool-record.sh select, plug, feather or render: the same
#   scripts/tool-record.sh rasterize, shrink, invert, inspect, cut, put, window, unwindow or
#                                    tighten: the same (plan m3, task D2)
#   scripts/tool-record.sh segment   the same, from SAM 3's answers, which the model tier's
#                                    segment_answers_the_records_examples_on_the_mac wrote
#                                    into target/tmp/model-tier/out/ first
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
  align | resize | tiles)
    scripts/thread-guard.sh cargo test --release -p whizbang-squint --test it -- \
      --ignored --exact "$1_record::write_the_record"
    ;;
  change_map | edge_similarity)
    scripts/thread-guard.sh cargo test --release -p whizbang-squint --test it -- \
      --ignored --exact compare_record::write_the_record
    ;;
  close | subtract | line | rect | stroke | paste | fill | mask_rect | protect | segment | select | plug | feather | render | \
    rasterize | shrink | invert | inspect | cut | put | window | unwindow | tighten)
    scripts/thread-guard.sh cargo test --release -p whizbang-rascal --test it -- \
      --ignored --exact "tool_records::writes::$1"
    ;;
  *)
    echo "tool-record: usage: scripts/tool-record.sh view|grow|align|resize|tiles|change_map|edge_similarity|close|subtract|line|rect|stroke|paste|fill|mask_rect|protect|segment|select|plug|feather|render|rasterize|shrink|invert|inspect|cut|put|window|unwindow|tighten" >&2
    exit 2
    ;;
esac
echo "tool-record: wrote docs/reference/tools/$1.md and docs/reference/tools/$1/"
}
main "$@"; exit
