#!/usr/bin/env bash
# `just deny` on sources: crates come from crates.io only (deny.toml, `[sources]`). The
# workspace as it is passes; a dependency planted from another registry fails, and so does
# one from a git source. Both are planted in a throwaway copy of the workspace's manifests,
# never in the workspace.
#
# Nothing here uses the network. The planted registry and the planted git source are folders
# of this test, named by `file://` addresses, and every proxy variable points at a port
# where nothing listens, so a step that reached for the network would fail and not hang.
# The workspace's own crates are read from cargo's home, where `just deny` put them: this
# test runs after it in `just check`.
#
# cargo gets a home of its own for the test, so that the planted sources leave nothing in
# the real one: the crates already downloaded are linked into it, read and never written.
source "$(dirname "$0")/lib.sh"

command -v cargo-deny >/dev/null || fail "cargo-deny is not installed (just setup names the command)"

# --- A copy of the manifests: the root's, each crate's, the lock file, deny.toml and the
# justfile. A crate's sources are empty files; `cargo metadata` wants them to exist.
copy_workspace() { # <to>
  local to=$1 manifest crate path
  mkdir -p "$to"
  cp Cargo.toml Cargo.lock deny.toml justfile rust-toolchain.toml "$to/"
  for manifest in crates/*/Cargo.toml; do
    crate=$(dirname "$manifest")
    mkdir -p "$to/$crate/src"
    cp "$manifest" "$to/$manifest"
    for path in src/lib.rs src/main.rs build.rs; do
      [ ! -f "$crate/$path" ] || : >"$to/$crate/$path"
    done
    # Targets the manifest names by path.
    sed -n 's/^path = "\(.*\)"$/\1/p' "$manifest" >"$tmp/paths"
    while IFS= read -r path; do
      mkdir -p "$to/$crate/$(dirname "$path")"
      : >"$to/$crate/$path"
    done <"$tmp/paths"
  done
}

# --- cargo's home for the test. The index, the downloaded crates and their unpacked
# sources of each registry already there are linked in by name; what the planted sources
# add lands beside the links, in this test's folder.
real_home=${CARGO_HOME:-$HOME/.cargo}
home="$tmp/cargo-home"
for part in index cache src; do
  mkdir -p "$home/registry/$part"
  for entry in "$real_home/registry/$part"/*; do
    [ -e "$entry" ] || continue
    ln -s "$entry" "$home/registry/$part/$(basename "$entry")"
  done
done

# deny <folder> <command...>: the command in that copy, with the test's cargo home and no
# way to the network. The toolchain is the one this repo pins, wherever the copy is.
toolchain=$(rustup show active-toolchain | cut -d' ' -f1)
deny() {
  local where=$1
  shift
  (cd "$where" && env CARGO_HOME="$home" RUSTUP_TOOLCHAIN="$toolchain" CARGO_NET_RETRY=0 \
    CARGO_HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 HTTP_PROXY=http://127.0.0.1:9 \
    https_proxy=http://127.0.0.1:9 http_proxy=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9 \
    all_proxy=http://127.0.0.1:9 NO_PROXY= no_proxy= "$@")
}

# --- The workspace as it is.
before=$(cat Cargo.toml Cargo.lock crates/*/Cargo.toml deny.toml | cksum)
copy_workspace "$tmp/as-it-is"
deny "$tmp/as-it-is" cargo deny check sources >"$tmp/out" 2>&1 || {
  cat "$tmp/out" >&2
  fail "the workspace as it is passes the check of sources (if a crate could not be fetched: this test uses no network; run \`just deny\` once to download the workspace's crates)"
}
grep -q -F "sources ok" "$tmp/out" || { cat "$tmp/out" >&2; fail "the check of sources says ok"; }
expect_pass "the workspace as it is, just deny" deny "$tmp/as-it-is" just deny

# --- The crate that is planted: one manifest and one empty source, under the licence the
# workspace uses, so that its source is the only thing wrong with it.
mkdir -p "$tmp/planted-crate/src"
cat >"$tmp/planted-crate/Cargo.toml" <<'MANIFEST'
[package]
name = "planted-crate"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
MANIFEST
: >"$tmp/planted-crate/src/lib.rs"

# --- A git source: a repository in this test's folder.
mkdir "$tmp/git-source"
cp -R "$tmp/planted-crate/." "$tmp/git-source/"
git -C "$tmp/git-source" init -q
git -C "$tmp/git-source" add Cargo.toml src/lib.rs
git -C "$tmp/git-source" -c user.name=test -c user.email=test@example.invalid \
  -c commit.gpgsign=false commit -q --no-verify -m "a planted crate"
copy_workspace "$tmp/from-git"
printf '\n[dependencies.planted-crate]\ngit = "file://%s"\n' "$tmp/git-source" \
  >>"$tmp/from-git/crates/whizbang-eval/Cargo.toml"
expect_fail "a dependency from a git source" "detected 'git' source not explicitly allowed" \
  deny "$tmp/from-git" cargo deny check sources
grep -q -F "sources FAILED" "$tmp/out" || { cat "$tmp/out" >&2; fail "a git source fails the check of sources"; }
grep -q -F "planted-crate" "$tmp/out" || { cat "$tmp/out" >&2; fail "the refusal names the planted crate"; }
expect_fail "a dependency from a git source, just deny" "sources FAILED" deny "$tmp/from-git" just deny
grep -q -F "licenses ok" "$tmp/out" || { cat "$tmp/out" >&2; fail "with a git source, only the source is wrong"; }

# --- Another registry: an index in this test's folder that offers the same crate, packed.
checksum() { # <file>: its SHA-256
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
registry="$tmp/registry"
mkdir -p "$registry/files" "$registry/index/pl/an" "$tmp/pack"
cp -R "$tmp/planted-crate" "$tmp/pack/planted-crate-0.1.0"
# COPYFILE_DISABLE: the tar of macOS would add a file of attributes, which cargo refuses.
COPYFILE_DISABLE=1 tar -czf "$registry/files/planted-crate-0.1.0.crate" -C "$tmp/pack" planted-crate-0.1.0
printf '{"dl":"file://%s/files/{crate}-{version}.crate"}\n' "$registry" >"$registry/index/config.json"
printf '{"name":"planted-crate","vers":"0.1.0","deps":[],"cksum":"%s","features":{},"yanked":false}\n' \
  "$(checksum "$registry/files/planted-crate-0.1.0.crate")" >"$registry/index/pl/an/planted-crate"
git -C "$registry/index" init -q
git -C "$registry/index" add config.json pl/an/planted-crate
git -C "$registry/index" -c user.name=test -c user.email=test@example.invalid \
  -c commit.gpgsign=false commit -q --no-verify -m "a planted index"
copy_workspace "$tmp/from-registry"
mkdir "$tmp/from-registry/.cargo"
printf '[registries.planted]\nindex = "file://%s/index"\n' "$registry" >"$tmp/from-registry/.cargo/config.toml"
printf '\n[dependencies.planted-crate]\nversion = "0.1.0"\nregistry = "planted"\n' \
  >>"$tmp/from-registry/crates/whizbang-eval/Cargo.toml"
expect_fail "a dependency from another registry" "detected 'registry' source not explicitly allowed" \
  deny "$tmp/from-registry" cargo deny check sources
grep -q -F "sources FAILED" "$tmp/out" || { cat "$tmp/out" >&2; fail "another registry fails the check of sources"; }
grep -q -F "planted-crate" "$tmp/out" || { cat "$tmp/out" >&2; fail "the refusal names the planted crate"; }
expect_fail "a dependency from another registry, just deny" "sources FAILED" deny "$tmp/from-registry" just deny
grep -q -F "licenses ok" "$tmp/out" || { cat "$tmp/out" >&2; fail "with another registry, only the source is wrong"; }

# --- The same crate from a path beside the workspace is no source at all: the plants above
# fail for where they come from, not for being there. (The crate graph refuses such a path;
# scripts/tests/test-crate-graph.sh.)
copy_workspace "$tmp/from-path"
printf '\n[dependencies.planted-crate]\npath = "%s"\nversion = "0.1.0"\n' "$tmp/planted-crate" \
  >>"$tmp/from-path/crates/whizbang-eval/Cargo.toml"
expect_pass "the planted crate by path passes the check of sources" deny "$tmp/from-path" cargo deny check sources

# --- Nothing was written to the real workspace: its manifests and lock file are as they were.
[ "$(cat Cargo.toml Cargo.lock crates/*/Cargo.toml deny.toml | cksum)" = "$before" ] ||
  fail "the test changed the workspace's own manifests or lock file"

finished
