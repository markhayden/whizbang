#!/usr/bin/env bash
# The rules an agent starts from: .claude/settings.json is valid and allows nothing in
# advance that publishes or bypasses a hook, and AGENTS.md still holds the fresh-clone
# checklist, the three roles and who merges. Nothing here runs an agent.
source "$(dirname "$0")/lib.sh"

settings=.claude/settings.json
rules=AGENTS.md

command -v jq >/dev/null || fail "jq is not installed"

# --- .claude/settings.json ---------------------------------------------------------------

# check_settings <file>: prints what is wrong with a settings file and fails, or passes.
check_settings() {
  local file=$1 bad
  jq -e . "$file" >/dev/null 2>&1 || { echo "settings: not valid JSON"; return 1; }
  jq -e '(.permissions.allow | type == "array" and length > 0)
    and (.permissions.deny | type == "array")
    and ([.permissions.allow[], .permissions.deny[], (.permissions.ask // [])[]]
      | all(type == "string" and test("^(Bash|Read)\\(.+\\)$")))' "$file" >/dev/null ||
    { echo "settings: permissions.allow and permissions.deny must be lists of Bash(...) or Read(...) rules"; return 1; }
  # Nothing that publishes, and no rule wide enough to cover something that does.
  bad=$(jq -r '.permissions.allow[]' "$file" |
    grep -E -i 'push|merge|tag|release|publish|no-verify|^Bash\((\*|git[ :]?\*|gh[ :]?\*|gh api[ :]?\*|cargo[ :]?\*|bash[ :]|sh[ :]|env[ :]|sudo[ :])' || true)
  [ -z "$bad" ] || { echo "settings: allowed in advance: $bad"; return 1; }
  jq -e '[.permissions.deny[]] | any(test("^Read\\(.*\\.env"))' "$file" >/dev/null ||
    { echo "settings: reading .env must be denied"; return 1; }
  for want in 'Bash(just *)' 'Bash(scripts/*)' 'Bash(git status *)' 'Bash(gh pr view *)'; do
    jq -e --arg want "$want" '.permissions.allow | index($want) != null' "$file" >/dev/null ||
      { echo "settings: must allow $want"; return 1; }
  done
}

expect_pass "the committed settings" check_settings "$settings"

# with_allow <rule>: the committed settings with one more allow rule.
with_allow() {
  jq --arg rule "$1" '.permissions.allow += [$rule]' "$settings" >"$tmp/settings.json"
  check_settings "$tmp/settings.json"
}
for rule in 'Bash(git push *)' 'Bash(git push:*)' 'Bash(gh pr merge *)' 'Bash(git merge *)' \
  'Bash(git commit --no-verify *)' 'Bash(git tag *)' 'Bash(gh release create *)' \
  'Bash(cargo publish *)' 'Bash(git *)' 'Bash(gh *)' 'Bash(gh api *)' 'Bash(*)'; do
  expect_fail "allowing $rule" "settings: allowed in advance: $rule" with_allow "$rule"
done

printf '{ "permissions": { "allow": [ ' >"$tmp/broken.json"
expect_fail "settings that are not JSON" "settings: not valid JSON" check_settings "$tmp/broken.json"

jq '.permissions.deny = []' "$settings" >"$tmp/no-deny.json"
expect_fail "settings that do not deny .env" "settings: reading .env must be denied" check_settings "$tmp/no-deny.json"

jq '.permissions.allow += ["git status"]' "$settings" >"$tmp/bare.json"
expect_fail "a rule that names no tool" "must be lists of Bash(...) or Read(...) rules" check_settings "$tmp/bare.json"

# --- AGENTS.md ---------------------------------------------------------------------------

# says <text>: AGENTS.md holds the text.
says() { grep -q -F -e "$1" "$rules" || fail "$rules must say: $1"; }

# The fresh-clone checklist is the first section, and holds its three steps in order.
[ "$(grep -m 1 '^## ' "$rules")" = "## On a fresh clone" ] ||
  fail "$rules: the first section must be: ## On a fresh clone"
checklist=$(awk '/^## / { on = ($0 == "## On a fresh clone") ; next } on' "$rules")
order=$(echo "$checklist" | grep -o -E 'just setup|WHIZBANG_WORDLIST|just scan' | awk '!seen[$0]++' | tr '\n' ' ')
[ "$order" = "just setup WHIZBANG_WORDLIST just scan " ] ||
  fail "$rules: the fresh-clone checklist must name, in order: just setup, WHIZBANG_WORDLIST, just scan"

for role in '**Builder:**' '**Reviewer:**' '**Orchestrator:**'; do says "$role"; done
says 'Only the orchestrator merges.'
says 'never a squash'
says 'Run `just check` before a commit and `just scan` before a push.'

# The word list is named by its variable only: no location.
if grep -n 'WHIZBANG_WORDLIST' "$rules" | grep -q -E '=|/'; then
  fail "$rules must not give a value or a location for WHIZBANG_WORDLIST"
fi

[ "$(cat CLAUDE.md)" = "@AGENTS.md" ] || fail "CLAUDE.md must be exactly: @AGENTS.md"
