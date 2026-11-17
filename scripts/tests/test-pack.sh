#!/usr/bin/env bash
# The pack's Python tests and how they are run (docs/plans/m1c-local-pack.md, task C1b, and
# the two rows of "Rules" on `uv` and on the pack's tests).
#
# - The runner, pack/tests/run.py, ends itself at its limit: shown with a test that blocks.
# - A test that leaves a thread that is not a daemon fails the run, and the run still ends.
# - scripts/test-pack.sh refuses to run outside scripts/thread-guard.sh.
# - `uv` is started offline, with no user's configuration, the three bounds and nothing
#   else of the caller's environment; the Python it names runs the runner, isolated.
#
# The runner's cases use the runner's own `python3` (3.9 or later, as test-baseline.sh does),
# so they need no `uv` and no pinned Python. The cases of scripts/test-pack.sh use a
# stand-in `uv` and a stand-in Python, so nothing here contacts anything or depends on what
# this machine has installed. Every process started here ends by itself.
source "$(dirname "$0")/lib.sh"

runner=pack/tests/run.py

# A test folder with one test that blocks, on an event nobody sets, far past the limit of 1
# second it is run with. It gives up after 30 seconds, so that a runner that has lost its
# limit fails this test (by time) and still ends.
mkdir "$tmp/blocks"
cat >"$tmp/blocks/test_blocks.py" <<'PY'
import threading
import unittest


class Blocks(unittest.TestCase):
    def test_blocks_past_the_limit(self):
        threading.Event().wait(30)
PY

# The runner ends itself at its limit, with the stack of what was running, and fails.
start=$(date +%s)
expect_fail "a test that blocks" "Timeout (0:00:01)!" \
  env WHIZBANG_PACK_TEST_LIMIT=1 python3 -I -B "$runner" "$tmp/blocks"
grep -F "test_blocks_past_the_limit" "$tmp/out" >/dev/null ||
  { cat "$tmp/out" >&2; fail "the runner names the test that was running at its limit"; }
took=$(($(date +%s) - start))
[ "$took" -lt 20 ] || fail "the runner ended $took seconds after a limit of 1"

# The limit is a number of seconds from 1 to 60; anything else is refused before any test.
for bad in 0 61 -1 abc "" 1.5; do
  expect_fail "the limit '$bad'" "WHIZBANG_PACK_TEST_LIMIT must be a whole number of seconds from 1 to 60" \
    env WHIZBANG_PACK_TEST_LIMIT="$bad" python3 -I -B "$runner" "$tmp/blocks"
done

# A thread that is not a daemon, left running by a test, fails the run; the run still ends.
mkdir "$tmp/lingers"
cat >"$tmp/lingers/test_lingers.py" <<'PY'
import threading
import unittest

STOP = threading.Event()
# The thread gives up after 30 seconds, so the process ends even if the runner did not end it.


class Lingers(unittest.TestCase):
    def test_leaves_a_thread(self):
        threading.Thread(target=STOP.wait, args=(30,), name="left-behind", daemon=False).start()
PY
expect_fail "a thread left behind" "a test left a thread that is not a daemon: left-behind" \
  env WHIZBANG_PACK_TEST_LIMIT=20 python3 -I -B "$runner" "$tmp/lingers"

# A failing test fails the run; a passing one passes it.
mkdir "$tmp/fails" "$tmp/passes"
printf 'import unittest\n\n\nclass F(unittest.TestCase):\n    def test_f(self):\n        self.fail("planted")\n' >"$tmp/fails/test_f.py"
printf 'import unittest\n\n\nclass P(unittest.TestCase):\n    def test_p(self):\n        pass\n' >"$tmp/passes/test_p.py"
expect_fail "a failing test" "planted" env WHIZBANG_PACK_TEST_LIMIT=20 python3 -I -B "$runner" "$tmp/fails"
expect_pass "a passing test" env WHIZBANG_PACK_TEST_LIMIT=20 python3 -I -B "$runner" "$tmp/passes"
# A folder with no test is a failure, never a pass.
mkdir "$tmp/empty"
expect_fail "no test" "no test was found" env WHIZBANG_PACK_TEST_LIMIT=20 python3 -I -B "$runner" "$tmp/empty"

# scripts/test-pack.sh refuses to run outside the guard, before it starts anything.
mkdir "$tmp/bin"
cat >"$tmp/bin/uv" <<'SH'
#!/bin/sh
# A stand-in `uv`: it writes down its arguments and its environment, then names the
# stand-in Python, which writes down its own arguments.
printf '%s\n' "$@" >"$PACK_TEST_RECORD/uv-args"
env | sort >"$PACK_TEST_RECORD/uv-env"
[ -z "$PACK_TEST_UV_FAILS" ] || { echo "uv: planted failure" >&2; exit 2; }
echo "$PACK_TEST_RECORD/python"
SH
chmod +x "$tmp/bin/uv"
mkdir "$tmp/record"
cat >"$tmp/record/python" <<SH
#!/bin/sh
printf '%s\n' "\$@" >"$tmp/record/python-args"
env | sort >"$tmp/record/python-env"
echo "the stand-in python ran"
SH
chmod +x "$tmp/record/python"

# The two variables the stand-in reads are not handed on by the script, which clears the
# environment; so in these cases they reach it only through a stand-in placed on the path
# that sets them itself.
cat >"$tmp/bin/uv-wrapper" <<SH
#!/bin/sh
PACK_TEST_RECORD="$tmp/record" PACK_TEST_UV_FAILS="\$(cat "$tmp/record/fails" 2>/dev/null || true)" exec "$tmp/bin/uv-real" "\$@"
SH
mv "$tmp/bin/uv" "$tmp/bin/uv-real"
mv "$tmp/bin/uv-wrapper" "$tmp/bin/uv"
chmod +x "$tmp/bin/uv"

expect_fail "outside the guard" "test-pack: refused: run it through scripts/thread-guard.sh" \
  env -u WHIZBANG_THREAD_GUARD PATH="$tmp/bin:$PATH" scripts/test-pack.sh
[ ! -e "$tmp/record/uv-args" ] || fail "test-pack started uv outside the guard"
# A variable that names the guard by hand switches nothing off: the process it names must
# be a guard of this repo above the script.
expect_fail "a guard named by hand" "test-pack: refused: run it through scripts/thread-guard.sh" \
  env PATH="$tmp/bin:$PATH" WHIZBANG_THREAD_GUARD="$$ 300 0.2" scripts/test-pack.sh
[ ! -e "$tmp/record/uv-args" ] || fail "test-pack started uv under a guard named by hand"

# Under the guard: uv is asked for the pinned Python, offline, with no configuration and
# the three bounds, in an environment cleared of the caller's variables.
pinned=$(cat pack/.python-version)
expect_pass "under the guard" \
  env PATH="$tmp/bin:$PATH" WHIZBANG_CALLERS_OWN=leaked UV_INDEX_URL=leaked \
  UV_PYTHON_INSTALL_DIR="$tmp/pythons" scripts/thread-guard.sh scripts/test-pack.sh
grep -F "the stand-in python ran" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "test-pack runs the Python uv names"; }
want_args="--no-config
--offline
--no-python-downloads
--managed-python
python
find
$pinned"
[ "$(cat "$tmp/record/uv-args")" = "$want_args" ] ||
  { cat "$tmp/record/uv-args" >&2; fail "uv is started as: $(echo $want_args)"; }
for var in UV_NO_CONFIG=1 UV_OFFLINE=1 UV_PYTHON_DOWNLOADS=never UV_MANAGED_PYTHON=1 \
  UV_CONCURRENT_DOWNLOADS=4 UV_CONCURRENT_INSTALLS=4 UV_CONCURRENT_BUILDS=1; do
  grep -x -F "$var" "$tmp/record/uv-env" >/dev/null ||
    { cat "$tmp/record/uv-env" >&2; fail "uv is given $var"; }
done
grep -x -F "UV_PYTHON_INSTALL_DIR=$tmp/pythons" "$tmp/record/uv-env" >/dev/null ||
  fail "uv is told where the caller's Pythons are installed, when the caller says"
# Nothing of the caller's but the allow-list: PATH, HOME, TMPDIR, UV_PYTHON_INSTALL_DIR.
names=$(sed 's/=.*//' "$tmp/record/uv-env" | grep -v -x -E 'PACK_TEST_RECORD|PACK_TEST_UV_FAILS|PWD|SHLVL|_|OLDPWD' | tr '\n' ' ')
for name in $names; do
  case "$name" in
    PATH | HOME | TMPDIR | UV_PYTHON_INSTALL_DIR | UV_NO_CONFIG | UV_OFFLINE | UV_PYTHON_DOWNLOADS | UV_MANAGED_PYTHON | \
      UV_CONCURRENT_DOWNLOADS | UV_CONCURRENT_INSTALLS | UV_CONCURRENT_BUILDS) ;;
    *) fail "uv is given a variable outside the allow-list: $name" ;;
  esac
done
want_py="-I
-B
$root/pack/tests/run.py"
[ "$(cat "$tmp/record/python-args")" = "$want_py" ] ||
  { cat "$tmp/record/python-args" >&2; fail "the Python runs the runner isolated, without bytecode"; }
grep -x -F "WHIZBANG_PACK_TEST_LIMIT=60" "$tmp/record/python-env" >/dev/null ||
  { cat "$tmp/record/python-env" >&2; fail "the runner is given its limit, 60 seconds"; }
grep -F "leaked" "$tmp/record/python-env" >/dev/null && fail "the Python is given the caller's variables"

# A uv that fails, or names no Python, fails the run and says what to do.
echo 1 >"$tmp/record/fails"
expect_fail "uv fails" "test-pack: uv found no Python $pinned" \
  env PATH="$tmp/bin:$PATH" scripts/thread-guard.sh scripts/test-pack.sh

# CI installs uv by an action pinned to a commit, and the pinned Python from GitHub's
# releases only (without the mirror, uv tries its maker's own host first), keyed in the
# cache by the pin. The same mirror is the one the refusal above names.
workflow=.github/workflows/ci.yml
mirror=https://github.com/astral-sh/python-build-standalone/releases/download
grep -E '^      - uses: taiki-e/install-action@[0-9a-f]{40}$' "$workflow" >/dev/null ||
  fail "CI installs uv by an action pinned to a commit"
grep -E '^          tool: .*(^|,| )uv@[0-9.]+($|,)' "$workflow" >/dev/null || fail "CI installs uv"
[ "$(grep -c -x -F "          UV_PYTHON_INSTALL_MIRROR: $mirror" "$workflow")" = 1 ] ||
  fail "CI installs the Python from GitHub's releases only: UV_PYTHON_INSTALL_MIRROR: $mirror"
grep -x -F '          uv python install --no-config --no-bin "$(cat pack/.python-version)"' "$workflow" >/dev/null ||
  fail "CI installs the Python the pack pins, with no configuration"
grep -x -F "          key: python-\${{ hashFiles('pack/.python-version') }}" "$workflow" >/dev/null ||
  fail "CI's cache is keyed by the pinned Python"
grep -x -F "          cache-directories: \${{ runner.temp }}/uv-python" "$workflow" >/dev/null &&
  grep -x -F "          UV_PYTHON_INSTALL_DIR: \${{ runner.temp }}/uv-python" "$workflow" >/dev/null ||
  fail "CI installs the Python into the folder it caches"
grep -F "UV_PYTHON_INSTALL_MIRROR=$mirror" "$tmp/out" >/dev/null ||
  { cat "$tmp/out" >&2; fail "the refusal names the install from GitHub's releases only"; }

finished
