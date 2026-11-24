#!/usr/bin/env bash
# scripts/gpu-host.sh (plan m6, task D1, item 3) with Modal's command and whizbang faked: its
# arguments and its refusals, and what it asks of Modal. Nothing here contacts a host, and no
# real `modal` or `whizbang` is run: both are scripts of this test, first on the PATH, which
# log what they were asked and answer from files the test writes.
source "$(dirname "$0")/lib.sh"

# A throwaway checkout of the script and the deployment, committed.
repo=$tmp/repo
mkdir -p "$repo/scripts" "$repo/deploy/modal"
cp scripts/gpu-host.sh "$repo/scripts/"
cp deploy/modal/sam3_host.py "$repo/deploy/modal/"
git -C "$repo" init -q
git -C "$repo" -c user.name=test -c user.email=test@example.invalid add -A
git -C "$repo" -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false commit -q -m "test: the deployment"
commit=$(git -C "$repo" rev-parse HEAD)
gh=$repo/scripts/gpu-host.sh

# The fakes.
bin=$tmp/bin
state=$tmp/state
mkdir -p "$bin" "$state"
cat >"$bin/modal" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_STATE/modal.log"
case "$1 $2" in
  "app list") [ -f "$FAKE_STATE/apps.json" ] && cat "$FAKE_STATE/apps.json" || exit 1 ;;
  "container list") [ -f "$FAKE_STATE/containers.json" ] && cat "$FAKE_STATE/containers.json" || exit 1
    # Each look at the containers lets one go, when the test asks for it.
    if [ -f "$FAKE_STATE/draining" ]; then echo '[]' >"$FAKE_STATE/containers.json"; fi ;;
  "app stop") ;;
  "deploy deploy/modal/sam3_host.py") echo "Created web function front => https://made-up-workspace--whizbang-sam3-front.modal.run" ;;
  "volume put") ;;
  *) echo "fake modal: not asked of it: $*" >&2; exit 9 ;;
esac
EOF
cat >"$bin/whizbang" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_STATE/whizbang.log"
case "$1 $2" in
  "models verify") cat "$FAKE_STATE/verify.txt" ;;
  "backends status") echo "segment	remote	remote	ready" ;;
  *) exit 9 ;;
esac
EOF
chmod +x "$bin/modal" "$bin/whizbang"
run() { env PATH="$bin:$PATH" FAKE_STATE="$state" WHIZBANG_GPU_HOST_LOOK_S=0 WHIZBANG_GPU_HOST_DOWN_S=3 "$gh" "$@"; }

config=$tmp/whizbang.toml
printf '[backends.segment]\nkind = "remote"\n\n[backends.segment.settings]\nprice_microusd_per_second = 300\nhost_limit_s = 600\n' >"$config"
out=$tmp/endpoint
apps() { printf '%s\n' "$1" >"$state/apps.json"; }
containers() { printf '%s\n' "$1" >"$state/containers.json"; }
deployed() { grep -F -x "deploy deploy/modal/sam3_host.py" "$state/modal.log" >/dev/null 2>&1; }
fresh() { rm -f "$state"/*.log "$state/draining" "$out"; }

# Arguments.
expect_fail "nothing to do" "say what to do" run
expect_fail "another command" "say what to do" run upload
expect_fail "a short commit" "40 lower-case hex digits" run deploy abc123 --config "$config" --endpoint-file "$out"
expect_fail "no config" "needs --config" run deploy "$commit" --endpoint-file "$out"
expect_fail "a relative config" "absolute path" run deploy "$commit" --config whizbang.toml --endpoint-file "$out"
expect_fail "no endpoint file" "needs --endpoint-file" run deploy "$commit" --config "$config"
expect_fail "an endpoint file in the repo" "outside the repo" run deploy "$commit" --config "$config" --endpoint-file "$repo/endpoint"
expect_fail "another commit" "is not at" run deploy "$(printf 'a%.0s' $(seq 40))" --config "$config" --endpoint-file "$out"

# The first deploy: an environment that holds no app of the name.
apps '[{"App ID": "ap-other", "Description": "another-app", "State": "deployed"}]'
containers '[]'
fresh
expect_pass "a first deploy" run deploy "$commit" --config "$config" --endpoint-file "$out"
deployed || fail "the first deploy deployed"
grep -F "made-up-workspace" "$tmp/out" >/dev/null && fail "the deploy printed the endpoint, which names the workspace"
[ "$(cat "$out")" = "https://made-up-workspace--whizbang-sam3-front.modal.run" ] || fail "the endpoint is written to its file"
grep -E -i "dict|secret|token" "$state/modal.log" >/dev/null && fail "deploy asks nothing of the claim store, a secret or a token"

# An app that is deployed, with no container running, is not stopped.
apps '[{"App ID": "ap-1", "Description": "whizbang-sam3", "State": "deployed"}]'
fresh
expect_fail "deployed with no container" "run scripts/gpu-host.sh down first" run deploy "$commit" --config "$config" --endpoint-file "$out"
deployed && fail "nothing is deployed beside a deployed app"
# Stopped, with a container left.
apps '[{"App ID": "ap-1", "Description": "whizbang-sam3", "State": "stopped"}]'
containers '[{"Container ID": "ta-1", "App ID": "ap-1"}]'
fresh
expect_fail "stopped with a container left" "container(s) of it are left" run deploy "$commit" --config "$config" --endpoint-file "$out"
deployed && fail "nothing is deployed beside a container left"
# A state it cannot read whole.
for listed in 'not json' '{"App ID": "ap-1"}' '[{"App ID": "ap-1", "Description": "whizbang-sam3"}]'; do
  apps "$listed"
  fresh
  expect_fail "a list it cannot read: $listed" "cannot be read whole" run deploy "$commit" --config "$config" --endpoint-file "$out"
  deployed && fail "nothing is deployed on a list it cannot read"
done
rm -f "$state/apps.json"
expect_fail "a list that cannot be asked" "cannot be read" run deploy "$commit" --config "$config" --endpoint-file "$out"
apps '[{"App ID": "ap-1", "Description": "whizbang-sam3", "State": "stopped"}]'
echo 'garbled' >"$state/containers.json"
fresh
expect_fail "containers it cannot read" "cannot be read whole" run deploy "$commit" --config "$config" --endpoint-file "$out"
# Stopped, with no container left: the deploy proceeds, and leaves the claim store as it was.
apps '[{"App ID": "ap-0", "Description": "whizbang-sam3", "State": "stopped"}, {"App ID": "ap-1", "Description": "whizbang-sam3", "State": "stopped"}]'
containers '[{"Container ID": "ta-9", "App ID": "ap-other"}]'
fresh
expect_pass "stopped with no container" run deploy "$commit" --config "$config" --endpoint-file "$out"
deployed || fail "a stopped app with no container is deployed again"
grep -E -i "dict" "$state/modal.log" >/dev/null && fail "the claim store is left as it was"

# The limits of one attempt: the config's host_limit_s must be their sum.
sed 's/host_limit_s = 600/host_limit_s = 601/' "$config" >"$tmp/other.toml"
fresh
expect_fail "another host limit" "come to 600" run deploy "$commit" --config "$tmp/other.toml" --endpoint-file "$out"
deployed && fail "nothing is deployed when the limits differ"
grep -v host_limit_s "$config" >"$tmp/none.toml"
expect_fail "no host limit" "names no host_limit_s" run deploy "$commit" --config "$tmp/none.toml" --endpoint-file "$out"
# A deployment whose table lacks a limit, or that asks for a retry, or that sets one apart.
for change in 's/    "scaledown_window": 60,//' 's/^HOST_LIMIT_S = 600/HOST_LIMIT_S = 540/' \
  's/max_containers=1, \*\*LIMITS)/max_containers=1, retries=2, **LIMITS)/' \
  's/max_containers=1, \*\*LIMITS)/max_containers=1, startup_timeout=300, timeout=240, scaledown_window=60)/'; do
  git -C "$repo" checkout -q -- .
  sed -i.bak -E "$change" "$repo/deploy/modal/sam3_host.py"
  rm "$repo/deploy/modal/sam3_host.py.bak"
  cmp -s "$repo/deploy/modal/sam3_host.py" deploy/modal/sam3_host.py && fail "the change made a difference: $change"
  git -C "$repo" -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false commit -q -a -m "test: changed"
  changed=$(git -C "$repo" rev-parse HEAD)
  fresh
  expect_fail "a deployment changed: $change" "in the deployment:" run deploy "$changed" --config "$config" --endpoint-file "$out"
  deployed && fail "nothing is deployed: $change"
  git -C "$repo" reset -q --hard "$commit"
done
# A checkout with changes.
echo "# changed" >>"$repo/deploy/modal/sam3_host.py"
expect_fail "a checkout with changes" "clean checkout" run deploy "$commit" --config "$config" --endpoint-file "$out"
git -C "$repo" checkout -q -- .

# weights: verified first, then the installation's folder copied to the same place.
models=$tmp/models
hex=$(printf '0%.0s' $(seq 64))
mkdir -p "$models/models/sam3/installations/$hex/files"
echo '{}' >"$models/models/sam3/installations/$hex/RECORD"
echo "whole sam3: sha256:$hex" >"$state/verify.txt"
fresh
expect_pass "weights" run weights --models-dir "$models"
grep -F -x "models verify sam3 --models-dir $models" "$state/whizbang.log" >/dev/null || fail "weights verifies sam3 first"
grep -F -x "volume put whizbang-models $models/models/sam3/installations/$hex /models/sam3/installations/$hex" "$state/modal.log" >/dev/null ||
  fail "weights copies the installation's folder into the volume"
echo "damaged sam3" >"$state/verify.txt"
fresh
expect_fail "weights of a damaged installation" "did not say the installation is whole" run weights --models-dir "$models"
[ -f "$state/modal.log" ] && fail "nothing is copied from an installation that is not whole"
expect_fail "weights with a relative folder" "absolute path" run weights --models-dir models

# probe hands the config and the env file to whizbang, and reads neither itself.
env_file=$tmp/whizbang.env
printf 'SEGMENT_TOKEN=made-up\n' >"$env_file"
fresh
expect_pass "probe" run probe --config "$config" --env-file "$env_file"
grep -F -x "backends status --probe --config $config --env-file $env_file" "$state/whizbang.log" >/dev/null || fail "probe is backends status --probe"
expect_fail "probe without an env file" "needs --config and --env-file" run probe --config "$config"

# down: stops the app, then waits until no container of it is left; fails while one is.
apps '[{"App ID": "ap-1", "Description": "whizbang-sam3", "State": "deployed"}]'
containers '[{"Container ID": "ta-1", "App ID": "ap-1"}]'
fresh
expect_fail "down with a container left" "are left after 3 seconds" run down
grep -F -x "app stop -y whizbang-sam3" "$state/modal.log" >/dev/null || fail "down stops the app"
fresh
touch "$state/draining"
expect_pass "down once the container goes" run down
expect_fail "down of no app" "lists no app" env PATH="$bin:$PATH" FAKE_STATE="$state" bash -c "echo '[]' >\"$state/apps.json\"; \"$gh\" down"

# The script never reads an env file or Modal's token file itself, and names no account.
grep -E '\.modal\.toml|source .*env|\. .*env_file' scripts/gpu-host.sh >/dev/null && fail "the script reads no env file and no token file"
grep -n -E "workspace\s*=|modal\.run/" deploy/modal/sam3_host.py >/dev/null && fail "the deployment names no workspace or endpoint"

finished
