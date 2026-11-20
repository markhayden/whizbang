#!/usr/bin/env bash
# The two workflows of a release (.github/workflows/prerelease.yml and release.yml; plan
# m4, task R2, items 2 and 5). Neither can run before a tag exists, so their shape is held
# here, line by line:
#
# - each runs on its kind of tag and on nothing else: no branch, no pull request, no
#   schedule, no button, no other workflow;
# - what each job runs is a closed list, compared whole: `prerelease` builds with `just
#   package` and nothing else builds; its `install` jobs run the bootstrap and the install
#   check and set up no Rust toolchain; `release` builds nothing;
# - no job runs in a container, starts a service or adds a package;
# - every job may only read, but `candidate`, and the one job of `release`, which write a
#   release and nothing else; no secret is named but the run's own token, in those;
# - nothing may be skipped or allowed to fail; the pinned tools are the repo's pins.
#
# Every rule is shown to fail on a copy with one thing changed. Nothing here runs a job.
source "$(dirname "$0")/lib.sh"

prerelease=.github/workflows/prerelease.yml
release=.github/workflows/release.yml

# code <workflow>: its lines without comments and empty lines.
code() { grep -v -E '^[[:space:]]*(#|$)' "$1" || true; }
# top <workflow> <key>: the lines of one top-level key, without its own line.
top() { code "$1" | awk -v head="$2:" '/^[^ ]/ { on = ($0 == head); next } on'; }
# jobs <workflow>: the name of each job, on one line.
jobs() { top "$1" jobs | sed -n 's/^  \([a-z][a-z-]*\):$/\1/p' | tr '\n' ' '; }
# job <workflow> <name>: the lines of one job.
job() { top "$1" jobs | awk -v head="  $2:" '/^  [a-z][a-z-]*:$/ { on = ($0 == head); next } on'; }
# runs <job's lines>: what it runs and uses, in order, one a line.
runs() { sed -n -e 's/^ *- uses: /uses: /p' -e 's/^ *run: /run: /p' -e 's/^ *- run: /run: /p' <<<"$1"; }

# common <workflow> <tag pattern>: what holds for both.
common() {
  local file=$1
  [ "$(top "$file" on)" = "  push:
    tags:
      - '$2'" ] || { echo "$file: it must run on a push of a tag '$2', and on nothing else"; return 1; }
  [ "$(top "$file" permissions)" = "  contents: read" ] ||
    { echo "$file: outside the job that writes a release, a job may read the repository and no more"; return 1; }
  if code "$file" | grep -E 'continue-on-error|^ *(- )?if:' >/dev/null; then
    echo "$file: nothing may continue on error or have a condition"; return 1
  fi
  if code "$file" | grep -E '^ *(container|services|image):|docker|apt-get|apt |brew |pip |npm |curl |wget |sudo ' >/dev/null; then
    echo "$file: no job runs in a container, starts a service, adds a package or downloads by hand"; return 1
  fi
  # A step's command is one line, so that the lists below are everything that runs.
  if code "$file" | grep -E '^ *(- )?run: *[|>]' >/dev/null; then
    echo "$file: a step's command is one line"; return 1
  fi
  if code "$file" | grep -E '^ *(shell|working-directory|defaults|env_file|pull_request_target):' >/dev/null; then
    echo "$file: no step names a shell or a folder of its own"; return 1
  fi
  # The one secret is the run's own token, where a release is written.
  if code "$file" | grep -F 'secrets.' >/dev/null; then
    echo "$file: no secret is named: no provider's key and no model reaches a hosted runner"; return 1
  fi
  if code "$file" | grep -F '${{' | grep -v -E '\$\{\{ (matrix\.os|runner\.temp|github\.ref|github\.ref_name|github\.token) \}\}' >/dev/null; then
    echo "$file: an expression names something else than the runner, the temp folder, the tag or the run's token"; return 1
  fi
}

check_prerelease() { # <workflow>
  local file=$1 build install candidate pins name
  common "$file" 'v[0-9]+.[0-9]+.[0-9]+-rc.[0-9]+' || return 1
  [ "$(jobs "$file")" = "build install candidate " ] || { echo "prerelease: the jobs must be build, install and candidate (found: $(jobs "$file"))"; return 1; }
  build=$(job "$file" build)
  install=$(job "$file" install)
  candidate=$(job "$file" candidate)
  for name in build install; do
    job "$file" "$name" | grep -x -F '        os: [macos-15, ubuntu-22.04]' >/dev/null &&
      job "$file" "$name" | grep -x -F '    runs-on: ${{ matrix.os }}' >/dev/null ||
      { echo "prerelease: $name must run on macos-15 and ubuntu-22.04"; return 1; }
  done
  # build: the tag held to the workspace's version, the toolchain, `just`, and the archive.
  [ "$(runs "$build")" = 'uses: actions/checkout@v4
run: scripts/tag-version.sh --candidate "$TAG" "$(sed -n '"'"'s/^version = "\(.*\)"$/\1/p'"'"' Cargo.toml | sed -n '"'"'1p'"'"')"
run: rustup show
uses: taiki-e/install-action@ab49d3c64680143e184118bed66a46419fc15519
run: just package
uses: actions/upload-artifact@v4' ] ||
    { echo "prerelease: build must check out, hold the tag to the workspace's version, set up the toolchain and just, run just package, and keep the archive; and nothing else"; return 1; }
  grep -x -F '          TAG: ${{ github.ref_name }}' <<<"$build" >/dev/null || { echo "prerelease: build must read the tag that started the run"; return 1; }
  grep -x -F '          name: archive-${{ matrix.os }}' <<<"$build" >/dev/null &&
    grep -x -F '            target/package/whizbang-*.tar.gz' <<<"$build" >/dev/null &&
    grep -x -F '            target/package/SHA256SUMS' <<<"$build" >/dev/null &&
    grep -x -F '          if-no-files-found: error' <<<"$build" >/dev/null ||
    { echo "prerelease: build must keep the archive and its sums line as its artifact, and fail without them"; return 1; }
  # install: nothing built, no toolchain; the bootstrap, then the install check.
  [ "$(runs "$install")" = 'uses: actions/checkout@v4
uses: taiki-e/install-action@ab49d3c64680143e184118bed66a46419fc15519
uses: actions/download-artifact@v4
run: scripts/install-prereqs.sh "$(command -v uv)" "$RUNNER_TEMP/prereqs"
run: scripts/install-check.sh "$RUNNER_TEMP"/candidate/whizbang-*.tar.gz "$RUNNER_TEMP/candidate/SHA256SUMS" "$RUNNER_TEMP/prereqs"' ] ||
    { echo "prerelease: install must check out, take uv and the archive, and run the bootstrap and then the install check; and nothing else"; return 1; }
  grep -x -F '    needs: build' <<<"$install" >/dev/null || { echo "prerelease: install must need build"; return 1; }
  grep -x -F '          name: archive-${{ matrix.os }}' <<<"$install" >/dev/null || { echo "prerelease: install must take the archive build made for its runner"; return 1; }
  if grep -E 'rustup|cargo|just|rust-cache' <<<"$install$candidate" >/dev/null; then
    echo "prerelease: only build sets up a Rust toolchain or builds"; return 1
  fi
  # candidate: after all four, both archives under one SHA256SUMS, as a draft.
  [ "$(runs "$candidate")" = 'uses: actions/checkout@v4
uses: actions/download-artifact@v4
uses: actions/download-artifact@v4
run: scripts/release-publish.sh join "$RUNNER_TEMP/files" "$RUNNER_TEMP/macos-15" "$RUNNER_TEMP/ubuntu-22.04"
run: scripts/release-publish.sh candidate "$TAG" "$RUNNER_TEMP/files"' ] ||
    { echo "prerelease: candidate must check out, take both archives, join their sums and make the candidate; and nothing else"; return 1; }
  grep -x -F '    needs: [build, install]' <<<"$candidate" >/dev/null || { echo "prerelease: candidate must need build and install, so that all four passed"; return 1; }
  grep -x -F '          name: archive-macos-15' <<<"$candidate" >/dev/null && grep -x -F '          name: archive-ubuntu-22.04' <<<"$candidate" >/dev/null ||
    { echo "prerelease: candidate must take the archive of each runner"; return 1; }
  # Leave to write: candidate, the repository's contents (a release), and nothing else.
  [ "$(grep -A1 -x -F '    permissions:' <<<"$candidate" | tail -n 1)" = "      contents: write" ] &&
    [ "$(code "$file" | grep -c -E '^ *permissions:')" = 2 ] && [ "$(code "$file" | grep -c -E ': *write')" = 1 ] &&
    [ "$(grep -c -E '^      [a-z-]+: (read|write|none)$' <<<"$candidate")" = 1 ] ||
    { echo "prerelease: only candidate may write, and only the repository's contents, which is where a release is"; return 1; }
  [ "$(code "$file" | grep -c -F 'github.token')" = 1 ] && grep -x -F '          GH_TOKEN: ${{ github.token }}' <<<"$candidate" >/dev/null ||
    { echo "prerelease: the run's token is given to candidate's last step and to nothing else"; return 1; }
  # No job keeps the token git was given at checkout.
  [ "$(code "$file" | grep -c -x -F '          persist-credentials: false')" = "$(code "$file" | grep -c -F 'actions/checkout@')" ] ||
    { echo "prerelease: every checkout must leave no credential behind"; return 1; }
  # The pinned tools are the repo's pins.
  pins=$(code "$file" | sed -n 's/^ *tool: //p' | tr ',\n' '  ')
  for name in $pins; do
    grep -x -F "${name%@*} ${name#*@}" scripts/tool-versions.txt >/dev/null ||
      { echo "prerelease: $name is not a pin of scripts/tool-versions.txt"; return 1; }
  done
  [ "$pins" = "just@$(awk '$1 == "just" { print $2 }' scripts/tool-versions.txt) uv@$(awk '$1 == "uv" { print $2 }' scripts/tool-versions.txt) " ] ||
    { echo "prerelease: build installs just and install installs uv, each at its pin, and no other tool"; return 1; }
}

check_release() { # <workflow>
  local file=$1 body
  common "$file" 'v[0-9]+.[0-9]+.[0-9]+' || return 1
  [ "$(jobs "$file")" = "release " ] || { echo "release: the one job must be release (found: $(jobs "$file"))"; return 1; }
  body=$(job "$file" release)
  grep -x -F '    runs-on: ubuntu-22.04' <<<"$body" >/dev/null || { echo "release: it must run on ubuntu-22.04"; return 1; }
  # It builds nothing: the gate, then the candidate's own files published.
  [ "$(runs "$body")" = 'uses: actions/checkout@v4
run: scripts/release-gate.sh "$TAG" "$RUNNER_TEMP/candidate"
run: scripts/release-publish.sh release "$TAG" "$RUNNER_TEMP/candidate"' ] ||
    { echo "release: it must check out, run the gate and then publish what the gate held; and nothing else"; return 1; }
  if code "$file" | grep -E 'rustup|cargo|just|install-action|rust-cache|artifact|matrix' >/dev/null; then
    echo "release: it builds nothing, sets up no tool and takes no artifact of a run"; return 1
  fi
  grep -x -F '          fetch-depth: 0' <<<"$body" >/dev/null && grep -x -F '          persist-credentials: false' <<<"$body" >/dev/null ||
    { echo "release: the checkout must hold every tag, and leave no credential behind"; return 1; }
  [ "$(grep -A2 -x -F '    permissions:' <<<"$body" | tail -n 2)" = "      contents: write
      actions: read" ] && [ "$(code "$file" | grep -c -E '^ *permissions:')" = 2 ] && [ "$(code "$file" | grep -c -E ': *write')" = 1 ] &&
    [ "$(grep -c -E '^      [a-z-]+: (read|write|none)$' <<<"$body")" = 2 ] ||
    { echo "release: the job may write the repository's contents (a release) and read the runs, and nothing else"; return 1; }
  [ "$(grep -c -x -F '          TAG: ${{ github.ref_name }}' <<<"$body")" = 2 ] && [ "$(grep -c -x -F '          GH_TOKEN: ${{ github.token }}' <<<"$body")" = 2 ] ||
    { echo "release: both steps must read the tag that started the run, with the run's token"; return 1; }
}

expect_pass "prerelease" check_prerelease "$prerelease"
expect_pass "release" check_release "$release"

# No other workflow runs on a tag, and the checks of a pull request keep their names.
[ "$(ls .github/workflows | tr '\n' ' ')" = "ci.yml prerelease.yml release.yml " ] || fail "the workflows are ci, prerelease and release"
[ "$(top .github/workflows/ci.yml on)" = "  pull_request:
  push:
    branches: [main]" ] || fail "ci runs on a pull request and a push to main, and on no tag"
[ "$(jobs .github/workflows/ci.yml)" = "check scripts release secrets ci " ] || fail "ci's jobs keep their names"
# The scripts the workflows run are there and can be run.
for name in tag-version install-prereqs install-check release-publish release-gate package; do
  [ -x "scripts/$name.sh" ] || fail "scripts/$name.sh is run by a workflow and must be there"
done

# changed <check> <file> <sed script> <expected text>: the file after the edit must fail.
changed() {
  sed "$3" "$2" >"$tmp/changed"
  cmp -s "$2" "$tmp/changed" && fail "the edit changed nothing: $3"
  expect_fail "$2 after: $3" "$4" "$1" "$tmp/changed"
}
# after <check> <file> <line, as a pattern> <line to add after it> <expected text>
after() {
  ADD="$4" awk -v pattern="$3" '{ print } $0 ~ pattern && !done { print ENVIRON["ADD"]; done = 1 }' "$2" >"$tmp/changed"
  cmp -s "$2" "$tmp/changed" && fail "nothing was added after: $3"
  expect_fail "$2 with: $4" "$5" "$1" "$tmp/changed"
}

for pair in "check_prerelease $prerelease" "check_release $release"; do
  set -- $pair
  # Another trigger: a branch, a pull request, a button, a schedule, every tag.
  after "$1" "$2" '^  push:$' '    branches: [main]' "and on nothing else"
  after "$1" "$2" '^on:$' '  pull_request:' "and on nothing else"
  after "$1" "$2" '^on:$' '  workflow_dispatch:' "and on nothing else"
  after "$1" "$2" '^on:$' '  schedule:' "and on nothing else"
  changed "$1" "$2" "s/^      - 'v.*'$/      - '*'/" "and on nothing else"
  changed "$1" "$2" "s/^      - 'v.*'$/      - 'v*'/" "and on nothing else"
  # Leave to write for every job, or more of it.
  changed "$1" "$2" 's/^  contents: read$/  contents: write/' "may read the repository and no more"
  after "$1" "$2" '^  contents: read$' '  packages: write' "may read the repository and no more"
  after "$1" "$2" '^      contents: write$' '      id-token: write' "may write"
  # Skipped, allowed to fail, in a container, with a package, a secret, or a block of commands.
  after "$1" "$2" '^    timeout-minutes: ' '    continue-on-error: true' "nothing may continue on error"
  after "$1" "$2" '^    timeout-minutes: ' "    if: github.actor == 'someone'" "nothing may continue on error or have a condition"
  after "$1" "$2" '^    timeout-minutes: ' '    container: ubuntu:22.04' "no job runs in a container"
  after "$1" "$2" '^    steps:$' '      - run: sudo apt-get install -y something' "adds a package"
  after "$1" "$2" '^    steps:$' '      - run: brew install something' "adds a package"
  after "$1" "$2" '^          TAG: ' '          KEY: ${{ secrets.FAL_KEY }}' "no secret is named"
  after "$1" "$2" '^          TAG: ' '          WHO: ${{ github.actor }}' "an expression names something else"
  after "$1" "$2" '^    steps:$' '      - run: |' "a step's command is one line"
  after "$1" "$2" '^    steps:$' '      - run: echo more' "and nothing else"
  after "$1" "$2" '^    steps:$' '      - uses: actions/setup-python@v5' "and nothing else"
done

# --- prerelease alone.
f=$prerelease
changed check_prerelease "$f" 's/^        run: just package$/        run: cargo build --release -p whizbang/' "and nothing else"
changed check_prerelease "$f" 's/^        run: just package$/        run: just ci/' "and nothing else"
changed check_prerelease "$f" '/^        run: scripts\/install-prereqs.sh /d' "the bootstrap and then the install check"
changed check_prerelease "$f" '/^        run: scripts\/install-check.sh /d' "the bootstrap and then the install check"
changed check_prerelease "$f" 's/^        run: scripts\/tag-version.sh --candidate /        run: scripts\/tag-version.sh /' "hold the tag to the workspace's version"
changed check_prerelease "$f" 's/^          TAG: ${{ github.ref_name }}$/          TAG: v0.1.0-rc.1/' "must read the tag that started the run"
# A toolchain or a build in the install job; the archive of the other runner.
awk '/^  install:$/ { on = 1 } on && /^      - uses: taiki-e/ { print "      - name: Rust toolchain"; print "        run: rustup show"; on = 0 } { print }' "$f" >"$tmp/toolchain"
expect_fail "install with a toolchain" "and nothing else" check_prerelease "$tmp/toolchain"
awk '/^  install:$/ { on = 1 } on && /^          tool: uv@/ { $0 = $0 ",just@1.59.0"; on = 0 } { print }' "$f" >"$tmp/just"
expect_fail "install with just" "only build sets up a Rust toolchain or builds" check_prerelease "$tmp/just"
awk '/^  install:$/ { on = 1 } on && /^          name: archive-/ { $0 = "          name: archive-macos-15"; on = 0 } { print }' "$f" >"$tmp/other"
expect_fail "install of another runner's archive" "must take the archive build made for its runner" check_prerelease "$tmp/other"
# One runner only, in either job.
for n in 1 2; do
  awk -v n="$n" '/^        os: \[macos-15, ubuntu-22.04\]$/ { seen++; if (seen == n) $0 = "        os: [ubuntu-22.04]" } { print }' "$f" >"$tmp/one"
  expect_fail "matrix $n on Linux alone" "must run on macos-15 and ubuntu-22.04" check_prerelease "$tmp/one"
done
changed check_prerelease "$f" 's/^    needs: build$/    needs: []/' "install must need build"
changed check_prerelease "$f" 's/^    needs: \[build, install\]$/    needs: [build]/' "candidate must need build and install"
changed check_prerelease "$f" 's/^          if-no-files-found: error$/          if-no-files-found: warn/' "fail without them"
# Leave to write in another job, another tool, a pin that is not the repo's, a kept credential.
after check_prerelease "$f" '^    needs: build$' '    permissions:
      contents: write' "only candidate may write"
changed check_prerelease "$f" 's/^      contents: write$/      contents: read/' "only candidate may write"
after check_prerelease "$f" '^        run: just package$' '        env:
          GH_TOKEN: ${{ github.token }}' "the run's token is given to candidate's last step"
changed check_prerelease "$f" 's/^          tool: just@.*$/          tool: just@1.0.0/' "is not a pin of scripts/tool-versions.txt"
changed check_prerelease "$f" 's/^          tool: uv@\(.*\)$/          tool: uv@\1,cargo-deny@0.20.2/' "only build sets up a Rust toolchain or builds"
changed check_prerelease "$f" 's/^          tool: just@\(.*\)$/          tool: just@\1,cargo-deny@0.20.2/' "and no other tool"
awk '/^          persist-credentials: false$/ && !done { done = 1; next } { print }' "$f" >"$tmp/kept"
expect_fail "a checkout that keeps its credential" "every checkout must leave no credential behind" check_prerelease "$tmp/kept"
changed check_prerelease "$f" 's/^  candidate:$/  publish:/' "the jobs must be build, install and candidate"

# --- release alone.
f=$release
after check_release "$f" '^    steps:$' '      - run: just package' "and nothing else"
after check_release "$f" '^      - uses: actions\/checkout@v4$' '      - uses: taiki-e/install-action@ab49d3c64680143e184118bed66a46419fc15519' "and nothing else"
changed check_release "$f" '/^        run: scripts\/release-gate.sh /d' "run the gate and then publish"
awk '/^        run: scripts\/release-gate.sh / { held = $0; next } { print } /^        run: scripts\/release-publish.sh / { print held }' "$f" >"$tmp/late"
expect_fail "the gate after the release" "run the gate and then publish" check_release "$tmp/late"
changed check_release "$f" 's/^          fetch-depth: 0$/          fetch-depth: 1/' "must hold every tag"
changed check_release "$f" 's/^      actions: read$/      actions: write/' "may write the repository's contents"
changed check_release "$f" 's/^    runs-on: ubuntu-22.04$/    runs-on: self-hosted/' "must run on ubuntu-22.04"
awk '/^          TAG: / && !done { $0 = "          TAG: v0.1.0"; done = 1 } { print }' "$f" >"$tmp/tag"
expect_fail "a tag written in" "must read the tag that started the run" check_release "$tmp/tag"

finished
