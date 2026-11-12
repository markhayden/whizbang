#!/usr/bin/env bash
# The rules an agent starts from.
#
# .claude/settings.json must hold exactly the rules listed below, and nothing else: a change
# to the settings needs the same change here, made on purpose. The rules only reduce prompts.
# They are NOT the boundary: the boundary is branch protection, the pre-push hook and CI.
# Whoever can edit the justfile or a script changes what an exact rule runs.
#
# The other files that can grant permissions are held too: no local settings or MCP server
# file is committed; a subagent file has only the keys name, description and tools, with the
# tools listed below; a skill is one SKILL.md whose frontmatter is name and description only;
# and no folder named .claude, at any depth, holds any file but the ones listed below.
#
# AGENTS.md must still hold the fresh-clone checklist, the push and merge rows, the three
# roles, who merges, the attribution rule, how the word list's variable is set and what may
# be passed on, the temp folder, the heavy commands and when a plan box is ticked, and must
# give no location for the word list.
# Nothing here runs an agent.
source "$(dirname "$0")/lib.sh"

settings=.claude/settings.json
rules=AGENTS.md

command -v jq >/dev/null || fail "jq is not installed"

# --- .claude/settings.json ---------------------------------------------------------------

cat >"$tmp/allow" <<'LIST'
Bash(just check)
Bash(just scan)
Bash(just scan-secrets)
Bash(just docs)
Bash(just docs-check)
Bash(just test)
Bash(just test-scripts)
Bash(just test-scripts-slow)
Bash(just test-timeout)
Bash(just search-canary)
Bash(just timing)
Bash(just fmt-check)
Bash(just clippy)
Bash(just graph)
Bash(just deny)
Bash(just deny-advisories)
Bash(just smoke)
Bash(just setup)
Bash(just ci)
Bash(just --list)
Bash(just scan-range *)
Bash(just scan-range-secrets *)
Bash(scripts/scan.sh)
Bash(scripts/scan.sh --secrets-only)
Bash(scripts/setup.sh --check)
Bash(scripts/docs.sh --check)
Bash(scripts/tests/run.sh)
Bash(scripts/check-crate-graph.sh)
Bash(scripts/timing.sh)
Bash(git status)
Bash(git status *)
Bash(git ls-files)
Bash(git ls-files *)
Bash(git rev-parse *)
Bash(git branch --list)
Bash(git branch --list -a)
Bash(git branch --list -r)
Bash(git worktree list)
Bash(git worktree list *)
Bash(git fetch)
Bash(git fetch origin)
Bash(git fetch origin main)
Bash(git ls-remote origin)
Bash(git ls-remote --heads origin)
Bash(gh pr view *)
Bash(gh pr checks *)
Bash(gh pr list *)
Bash(gh pr diff *)
Bash(gh run view *)
Bash(gh run list *)
LIST
cat >"$tmp/ask" <<'LIST'
Bash(*git push*)
Bash(*git merge*)
Bash(*git tag*)
Bash(*gh pr merge*)
Bash(*gh release*)
Bash(*cargo publish*)
Bash(gh * -R*)
Bash(gh * --repo*)
Bash(gh *://*)
Bash(gh *github.com*)
LIST
cat >"$tmp/deny" <<'LIST'
Bash(*--no-verify*)
Read(./.env)
Read(./.env.*)
Read(**/.env)
Read(**/.env.*)
LIST

# check_settings <file>: prints what is wrong with a settings file and fails, or passes.
check_settings() {
  local file=$1 list
  jq -e . "$file" >/dev/null 2>&1 || { echo "settings: not valid JSON"; return 1; }
  [ "$(jq -c 'keys' "$file")" = '["$schema","permissions"]' ] ||
    { echo "settings: the only top-level keys are \$schema and permissions"; return 1; }
  [ "$(jq -c '.permissions | keys' "$file" 2>/dev/null)" = '["allow","ask","deny"]' ] ||
    { echo "settings: permissions holds allow, ask and deny, and nothing else"; return 1; }
  for list in allow ask deny; do
    jq -e --arg list "$list" '.permissions[$list] | type == "array" and all(type == "string")' "$file" >/dev/null ||
      { echo "settings: permissions.$list must be a list of rule strings"; return 1; }
    jq -r --arg list "$list" '.permissions[$list][]' "$file" >"$tmp/found"
    if ! diff "$tmp/$list" "$tmp/found" >"$tmp/diff"; then
      echo "settings: permissions.$list is not the list in scripts/tests/test-rules.sh"
      cat "$tmp/diff"
      return 1
    fi
  done
}

expect_pass "the committed settings" check_settings "$settings"

# Each `just <recipe>` rule names a recipe the justfile has.
grep -o '^Bash(just [a-z][a-z-]*' "$tmp/allow" | sed 's/^Bash(just //' | sort -u | while read -r name; do
  grep -q -E "^$name( [a-z]+)*:" justfile || fail "the settings allow just $name, which the justfile does not have"
done
# Each `scripts/...` rule names a script that is there.
grep -o '^Bash(scripts/[^ )]*' "$tmp/allow" | sed 's/^Bash(//' | sort -u | while read -r path; do
  [ -x "$path" ] || fail "the settings allow $path, which is not an executable file"
done

# mutated <jq filter>: the committed settings after the filter.
mutated() {
  jq "$1" "$settings" >"$tmp/settings.json"
  check_settings "$tmp/settings.json"
}
for rule in 'Bash(g*)' 'Bash(git commit *)' 'Bash(python3 *)' 'Read(//**)' 'Bash(just *)' \
  'Bash(scripts/*)' 'Bash(git push *)' 'Bash(gh pr merge *)' 'Bash(git diff *)' \
  'Bash(gh api repos/*)' 'Bash(scripts/heavy.sh *)' 'Bash(git branch --list *)' 'Bash(*)'; do
  expect_fail "allowing $rule" "permissions.allow is not the list" \
    mutated ".permissions.allow += [\"$rule\"]"
done
expect_fail "an allow rule removed" "permissions.allow is not the list" mutated 'del(.permissions.allow[0])'
expect_fail "ask emptied" "permissions.ask is not the list" mutated '.permissions.ask = []'
expect_fail "an ask rule removed" "permissions.ask is not the list" mutated 'del(.permissions.ask[0])'
expect_fail "no ask for a pull request by its address" "permissions.ask is not the list" \
  mutated '.permissions.ask -= ["Bash(gh *://*)"]'
expect_fail "no ask for another host's address" "permissions.ask is not the list" \
  mutated '.permissions.ask -= ["Bash(gh *github.com*)"]'
expect_fail "deny emptied" "permissions.deny is not the list" mutated '.permissions.deny = []'
expect_fail "no ask list" "nothing else" mutated 'del(.permissions.ask)'
expect_fail "a default mode" "nothing else" mutated '.permissions.defaultMode = "bypassPermissions"'
expect_fail "more folders" "nothing else" mutated '.permissions.additionalDirectories = ["/"]'
expect_fail "a hooks block" "only top-level keys" mutated '.hooks = {"PreToolUse": []}'
expect_fail "an env block" "only top-level keys" mutated '.env = {"A": "b"}'
expect_fail "a sandbox block" "only top-level keys" mutated '.sandbox = {"enabled": false}'
expect_fail "every MCP server switched on" "only top-level keys" mutated '.enableAllProjectMcpServers = true'
expect_fail "a list that is not a list" "must be a list of rule strings" mutated '.permissions.allow = "Bash(*)"'

printf '{ "permissions": { "allow": [ ' >"$tmp/broken.json"
expect_fail "settings that are not JSON" "settings: not valid JSON" check_settings "$tmp/broken.json"

# --- Other files that can grant permissions ----------------------------------------------

# Local settings and MCP servers are never committed. A subagent file holds only the keys
# below, with exactly the tools listed here, so it cannot choose a permission mode, a model,
# hooks or servers. A skill is one file, .claude/skills/<name>/SKILL.md, whose frontmatter
# is `name` and `description` and nothing else: no `allowed-tools`, no model, no hooks; and
# it runs no command when it is loaded (no `!` before a backtick, and no block fenced with
# three backticks or three tildes and a `!`, inside a `>` quote or not). Nothing else may
# be in a folder named .claude, at any depth and in any letter case: no commands, no hooks,
# no settings or skills of a folder further down; and no path is itself named .claude
# without being a folder (a link to one somewhere else). A change to any of this needs the same change here.
builder_tools="Bash, Read, Edit, Write, Grep, Glob"
reviewer_tools="Bash, Read, Grep, Glob"

# front <file>: the lines of its frontmatter; nothing when the file does not start with one.
front() { awk 'NR == 1 { if ($0 != "---") exit; next } $0 == "---" { exit } { print }' "$1"; }
# front_keys <file>: the keys, in order, each followed by a space.
front_keys() { front "$1" | sed -n 's/^\([A-Za-z][A-Za-z0-9_-]*\):.*$/\1/p' | tr '\n' ' '; }
# front_odd <file>: how many frontmatter lines are not one key with its value on the line.
front_odd() { front "$1" | grep -c -v -E '^[A-Za-z][A-Za-z0-9_-]*:[[:space:]]*[^[:space:]>|].*$' || true; }

# granted <folder>: prints what in a checkout grants permissions outside settings.json.
granted() {
  (
    cd "$1"
    git ls-files -- .claude/settings.local.json .mcp.json '**/.mcp.json' | sed 's/^/tracked: /'
    grep -l -i -E '^[[:space:]]*permissionMode[[:space:]]*:' .claude/agents/*.md 2>/dev/null | sed 's/^/permission mode: /'
    git -c core.excludesFile=/dev/null ls-files --cached --others --exclude-standard | { grep -i -E '(^|/)\.claude(/|$)' || true; } | while IFS= read -r f; do
      tools=
      if printf '%s\n' "$f" | grep -i -E '(^|/)\.claude$' >/dev/null; then echo "named .claude and not a folder: $f"; continue; fi
      case "$f" in
        .claude/settings.json) continue ;;
        .claude/agents/builder.md) keys="name description tools " tools=$builder_tools ;;
        .claude/agents/reviewer.md) keys="name description tools " tools=$reviewer_tools ;;
        .claude/skills/kickoff/SKILL.md | .claude/skills/port-module/SKILL.md | .claude/skills/review-merge/SKILL.md)
          keys="name description "
          ;;
        *) echo "in a .claude folder, and not a file this test knows: $f"; continue ;;
      esac
      [ -f "$f" ] && [ ! -L "$f" ] || { echo "not a plain file: $f"; continue; }
      [ "$(front_odd "$f")" = 0 ] || echo "a frontmatter line that is not one key and its value: $f"
      [ "$(front_keys "$f")" = "$keys" ] || echo "frontmatter keys: $f has [$(front_keys "$f")], expected [$keys]"
      case "$f" in
        .claude/agents/*)
          [ "$(front "$f" | sed -n 's/^tools:[[:space:]]*//p')" = "$tools" ] || echo "tools: $f must list exactly: $tools"
          ;;
        *)
          name=${f#.claude/skills/}
          name=${name%/SKILL.md}
          [ "$(front "$f" | sed -n 's/^name:[[:space:]]*//p')" = "$name" ] || echo "skill name: $f must be named $name, as its folder is"
          if grep -q -E -e '!`|^[[:space:]>]*(```+|~~~+)[[:space:]]*!' "$f"; then echo "runs a command when it is loaded: $f"; fi
          ;;
      esac
    done
  ) >"$tmp/granted"
  [ ! -s "$tmp/granted" ] || { cat "$tmp/granted"; return 1; }
}
expect_pass "nothing else grants permissions" granted .

git init -q "$tmp/repo"
mkdir -p "$tmp/repo/.claude/agents"
cp .claude/agents/*.md "$tmp/repo/.claude/agents/"
mkdir -p "$tmp/repo/.claude/skills"
cp -R .claude/skills/. "$tmp/repo/.claude/skills/"
git -C "$tmp/repo" add .claude
expect_pass "a copy of the agent files and the skills" granted "$tmp/repo"
[ -n "$(git -C "$tmp/repo" ls-files -- .claude/skills)" ] || fail "the copy holds the skills"
echo '{}' >"$tmp/repo/.claude/settings.local.json"
git -C "$tmp/repo" add -f .claude/settings.local.json
expect_fail "local settings committed" "tracked: .claude/settings.local.json" granted "$tmp/repo"
git -C "$tmp/repo" rm -q -f .claude/settings.local.json
echo '{}' >"$tmp/repo/.mcp.json"
git -C "$tmp/repo" add -f .mcp.json
expect_fail "MCP servers committed" "tracked: .mcp.json" granted "$tmp/repo"
git -C "$tmp/repo" rm -q -f .mcp.json

# planted <name> <expected text> <file> <sed script>: the copy with one file edited must fail,
# saying the text. The file is put back afterwards.
planted() {
  local name=$1 text=$2 file=$3 script=$4
  sed "$script" "$file" >"$tmp/repo/$file"
  cmp -s "$file" "$tmp/repo/$file" && fail "the edit changed nothing: $name"
  expect_fail "$name" "$text" granted "$tmp/repo"
  cp "$file" "$tmp/repo/$file"
}
skill=.claude/skills/kickoff/SKILL.md
for line in 'allowed-tools: Bash(git push *)' 'allowed-tools: Bash' 'model: a-model' 'hooks: {}' \
  'context: fork' 'agent: builder' 'disable-model-invocation: false' 'Allowed-Tools: Bash'; do
  planted "a skill with the line: $line" "frontmatter keys: $skill" "$skill" "2s/^/$line\\
/"
done
planted "a skill with a key after its description" "frontmatter keys: $skill" "$skill" '3s/$/\
allowed-tools: Bash/'
planted "a skill with tools on continued lines" "a frontmatter line that is not one key and its value: $skill" "$skill" '3s/$/\
allowed-tools:\
  - Bash/'
planted "a skill with a description that runs on" "a frontmatter line that is not one key and its value: $skill" "$skill" \
  '3s/^description: .*$/description: >\
  words/'
planted "a skill with no name" "frontmatter keys: $skill" "$skill" '2d'
planted "a skill named for another" "skill name: $skill must be named kickoff" "$skill" '2s/kickoff/review-merge/'
planted "a skill with no frontmatter" "frontmatter keys: $skill" "$skill" '1d'
planted "a skill that runs a command when loaded" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
!`git push`/'
planted "a skill with a block that runs when loaded" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
```!\
git push\
```/'
planted "a skill with such a block, spaced" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
  ```` !\
git push\
````/'
planted "a skill with such a block, fenced with tildes" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
~~~!\
git push\
~~~/'
planted "a skill with such a block, tildes and spaces" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
 ~~~~ !\
git push\
~~~~/'
planted "a skill with such a block inside a quote" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
> ```!\
> git push\
> ```/'
planted "a skill with such a block inside a quote in a quote" "runs a command when it is loaded: $skill" "$skill" '$s/$/\
>> ~~~!\
>> git push\
>> ~~~/'

# other <path> [<text>]: a file at the path, tracked or not, must fail, naming the path.
other() {
  mkdir -p "$tmp/repo/$(dirname "$1")"
  printf '%s\n' "${2:-words}" >"$tmp/repo/$1"
  expect_fail "an untracked $1" "in a .claude folder, and not a file this test knows: $1" granted "$tmp/repo"
  git -C "$tmp/repo" add -f -- "$1"
  expect_fail "a tracked $1" "in a .claude folder, and not a file this test knows: $1" granted "$tmp/repo"
  git -C "$tmp/repo" rm -q -f --cached -- "$1"
  rm "$tmp/repo/$1"
}
other .claude/skills/kickoff/run.sh 'git push'
other .claude/skills/kickoff/inner/SKILL.md
other .claude/skills/another/SKILL.md
other .claude/skills/SKILL.md
other .claude/commands/ship.md 'allowed-tools: Bash(git push *)'
other .claude/hooks/pre-tool.sh
other .claude/settings.toml
other .claude/CLAUDE.md
other .claude/rules/more.md
other .claude/output-styles/loud.md
other sub/.claude/skills/kickoff/SKILL.md
other sub/.claude/settings.json '{}'
other sub/deeper/.claude/agents/builder.md
other crates/.claude/commands/x.md
# In another letter case, which is the same folder where the file system ignores case.
other docs/.CLAUDE/commands/x.md
# A path that is itself named .claude and is no folder: a link to one somewhere else.
for link in linked/.claude linked/deeper/.CLAUDE; do
  mkdir -p "$tmp/repo/$(dirname "$link")"
  [ ! -e "$tmp/repo/$link" ] || fail "$link is already there"
  ln -s "$root/.claude" "$tmp/repo/$link"
  [ -L "$tmp/repo/$link" ] || fail "$link is not a link"
  expect_fail "an untracked link at $link" "named .claude and not a folder: $link" granted "$tmp/repo"
  git -C "$tmp/repo" add -f -- "$link"
  expect_fail "a tracked link at $link" "named .claude and not a folder: $link" granted "$tmp/repo"
  git -C "$tmp/repo" rm -q -r -f --cached -- "$link"
  rm "$tmp/repo/$link"
done
expect_pass "a folder whose name only starts the same" bash -c 'mkdir -p "$1/.claude-notes" && echo words >"$1/.claude-notes/a.md"' - "$tmp/repo"
expect_pass "is no .claude folder" granted "$tmp/repo"
rm -r "$tmp/repo/.claude-notes"

agent=.claude/agents/reviewer.md
planted "a reviewer that can edit" "tools: $agent must list exactly: $reviewer_tools" "$agent" '/^tools:/s/$/, Edit/'
planted "a reviewer with every tool" "tools: $agent must list exactly" "$agent" '/^tools:/s/:.*/: "*"/'
planted "a reviewer with no tools line" "frontmatter keys: $agent" "$agent" '/^tools:/d'
planted "a builder with one more tool" "tools: .claude/agents/builder.md must list exactly: $builder_tools" \
  .claude/agents/builder.md '/^tools:/s/$/, WebFetch/'
for line in 'model: a-model' 'skills: kickoff' 'mcpServers: {}' 'hooks: {}' 'disallowedTools: Read' \
  'permissionMode: acceptEdits' 'tools: Bash'; do
  planted "a subagent with the line: $line" "frontmatter keys: $agent" "$agent" "2s/^/$line\\
/"
done
other .claude/agents/another.md
expect_pass "the copy, put back" granted "$tmp/repo"

awk '{ print } /^name: reviewer$/ { print "permissionMode: bypassPermissions" }' \
  .claude/agents/reviewer.md >"$tmp/repo/.claude/agents/reviewer.md"
grep -q '^permissionMode' "$tmp/repo/.claude/agents/reviewer.md" || fail "the planted permission mode is not there"
expect_fail "a subagent with a permission mode" "permission mode: .claude/agents/reviewer.md" granted "$tmp/repo"

# --- The two recipes that take arguments -------------------------------------------------

# `just scan-range *` is allowed, so what follows the recipe's name must reach the scan as
# two plain arguments and never be read by a shell.
command -v just >/dev/null || fail "just is not installed"
for recipe in scan-range scan-range-secrets; do
  for planted in "\$(touch $tmp/made)" "\`touch $tmp/made\`" "x\"; touch $tmp/made; \"" "x'; touch $tmp/made; '"; do
    just "$recipe" "$planted" HEAD >/dev/null 2>&1 || true
    just "$recipe" HEAD "$planted" >/dev/null 2>&1 || true
    [ ! -e "$tmp/made" ] || fail "just $recipe ran a command planted in an argument: $planted"
  done
done

# --- AGENTS.md ---------------------------------------------------------------------------

# check_rules <file>: prints what the rules file is missing and fails, or passes.
check_rules() {
  local file=$1 checklist order row builder text
  [ "$(grep -m 1 '^## ' "$file")" = "## On a fresh clone" ] ||
    { echo "rules: the first section must be: ## On a fresh clone"; return 1; }
  checklist=$(awk '/^## / { on = ($0 == "## On a fresh clone") ; next } on' "$file")
  order=$(echo "$checklist" | grep -o -E 'just setup|WHIZBANG_WORDLIST|just scan' | awk '!seen[$0]++' | tr '\n' ' ')
  [ "$order" = "just setup WHIZBANG_WORDLIST just scan " ] ||
    { echo "rules: the fresh-clone checklist must name, in order: just setup, WHIZBANG_WORDLIST, just scan"; return 1; }

  row=$(grep '^| Push |' "$file" || true)
  for text in '`just check` passed on exactly the tree being pushed' '`git status --short` prints nothing after the commit' \
    '`just scan`' 'pre-push hook' 'Never push with `--no-verify`'; do
    case "$row" in *"$text"*) ;; *) echo "rules: the Push row must say: $text"; return 1 ;; esac
  done
  row=$(grep '^| Merge |' "$file" || true)
  for text in 'CI green on both runners' '`just scan-range <base> <head>`' 'the exact head that was reviewed' \
    '| The orchestrator, and nobody else |'; do
    case "$row" in *"$text"*) ;; *) echo "rules: the Merge row must say: $text"; return 1 ;; esac
  done

  builder=$(awk '/^- \*\*Builder:\*\*/ { on = 1 } on && /^- \*\*Reviewer:\*\*/ { on = 0 } on' "$file")
  case "$builder" in *'Pushes'*'opens the pull request'*'Never merges a pull'*'request or into `main`, never pushes to `main`, never tags.'*) ;; *)
    echo "rules: the builder pushes its branch, opens the pull request and never merges"; return 1 ;; esac
  for text in '- **Reviewer:**' '- **Orchestrator:**' 'Only the orchestrator merges.' 'never a squash' \
    'Run `just check` before a commit, and `just check` and `just scan` before a push.' \
    'Add `Co-Authored-By`, "Generated with", or any agent or model name to a commit, a pull' \
    'They are not the boundary.' \
    'The path may be given to another agent working in this repo.' \
    'never committed, opened, quoted, printed or copied' \
    'as an `env` entry in `.claude/settings.local.json`, which is never committed' \
    'and what it holds is never passed on.' \
    'is not one of the exact allowed commands' \
    '- **The system temp folder,**' \
    '`gh pr diff <n> --name-only` lists the files an open pull request changes' \
    'These are heavy:' \
    'The path is never written into a hand-back, a pull request, a commit or a tracked file.' \
    'The local settings file that may hold it is never committed.' \
    'are a tripwire, not a proof' \
    'Every change to these files is read in full by the reviewer and by the orchestrator.' \
    'the clean-room scan prints a count and names nothing' \
    'is ticked by a later pull request'; do
    grep -q -F -e "$text" "$file" || { echo "rules: must say: $text"; return 1; }
  done

  # The word list is named by its variable only, anywhere in the file.
  if grep -q -E 'WHIZBANG_WORDLIST[=:]|~/|\$HOME|/Users/|/home/' "$file"; then
    echo "rules: must give no value or location for WHIZBANG_WORDLIST, and no home path"
    return 1
  fi
}

expect_pass "the committed rules" check_rules "$rules"

# changed <sed script> <expected text>: the rules after the edit must fail, saying the text.
changed() {
  sed "$1" "$rules" >"$tmp/rules.md"
  cmp -s "$rules" "$tmp/rules.md" && fail "the edit changed nothing: $1"
  expect_fail "rules after: $1" "$2" check_rules "$tmp/rules.md"
}
changed 's/| The orchestrator, and nobody else |/| Anyone |/' "the Merge row must say"
changed '/^| Merge |/d' "the Merge row must say"
changed '/^| Push |/s/`just scan`/the scans/' "the Push row must say"
changed '/^| Push |/s/ Never push with `--no-verify`//' "the Push row must say"
changed '/^- Add `Co-Authored-By`/d' 'must say: Add `Co-Authored-By`'
changed 's/Only the orchestrator merges\.//' "must say: Only the orchestrator merges."
changed 's/Never merges a pull$/Never pushes a pull/' "the builder pushes its branch"
changed '/^| Push |/s/ passed on exactly the tree being pushed//' "the Push row must say"
changed 's/^- \*\*Reviewer:\*\*/- Reviewer:/' "must say: - **Reviewer:**"
changed 's/^- \*\*Orchestrator:\*\*/- Orchestrator:/' "must say: - **Orchestrator:**"
changed 's/They are not the boundary\.//' "must say: They are not the boundary."
changed 's/^1\. Run `just setup`/1. Run the setup/' "the fresh-clone checklist must name"
changed 's/The path may be given to another agent working in this repo\.//' "must say: The path may be given"
changed 's/never committed, opened, quoted, printed or copied/kept/' "must say: never committed, opened"
changed 's/^- \*\*The system temp folder,\*\*/- The system temp folder,/' "must say: - **The system temp folder,**"
changed 's/These are heavy:/Some are heavy:/' "must say: These are heavy:"
changed 's/that may hold it is never committed\./that may hold it is committed./' "must say: The local settings file"
changed 's/are a tripwire, not a proof/are the proof/' "must say: are a tripwire, not a proof"
changed 's/`\.claude\/settings\.local\.json`, which is never committed/`.claude\/settings.local.json`, which is committed/' "must say: as an \`env\` entry"
changed 's/and what it holds is never passed on\./and what it holds is passed on with care./' "must say: and what it holds is never passed on."
changed 's/^   The path is never written into a hand-back/   The path is written into a hand-back/' "must say: The path is never written"
changed 's/is ticked by a later pull request/is ticked at once/' "must say: is ticked by a later pull request"
changed 's|a private word list the owner provides|~/words|' "no value or location"
changed '$s|$| WHIZBANG_WORDLIST=words|' "no value or location"

[ "$(cat CLAUDE.md)" = "@AGENTS.md" ] || fail "CLAUDE.md must be exactly: @AGENTS.md"

# --- Private locations -------------------------------------------------------------------

# No document, settings file, workflow, recipe or script names a folder under a home
# directory or on another volume. (The secrets scan finds absolute home paths; this finds
# the `~`, `~user`, `$HOME` and volume forms.) The script tests are left out: they plant
# such paths on purpose.
homes='(^|[^A-Za-z0-9_.])~[A-Za-z0-9_-]*/|\$HOME/|\$\{HOME\}|/Volumes/'
# home_paths <folder>: prints where a tracked file of a checkout holds such a path.
home_paths() {
  if git -C "$1" -c grep.threads=1 grep -n -I -E -e "$homes" -- '*.md' '*.toml' '*.json' '*.yml' justfile scripts .claude \
    ':!scripts/tests' >"$tmp/homes"; then
    cut -d: -f1,2 "$tmp/homes"
    return 1
  fi
}
expect_pass "no home path in a tracked file" home_paths .

n=0
for planted in 'in ~/owner/x' 'in ~mark/x' 'in $HOME/x' 'in ${HOME}/x' 'in /Volumes/disk/x'; do
  for name in notes.md justfile scripts/a.sh; do
    n=$((n + 1))
    git init -q "$tmp/h$n"
    mkdir -p "$tmp/h$n/scripts"
    printf '%s\n' "$planted" >"$tmp/h$n/$name"
    git -C "$tmp/h$n" add "$name"
    expect_fail "$planted in $name" "$name:1" home_paths "$tmp/h$n"
  done
done
git init -q "$tmp/plain"
printf '%s\n' 'about ~3 minutes; a/~b; see docs/x' >"$tmp/plain/notes.md"
git -C "$tmp/plain" add notes.md
expect_pass "a tilde that is not a path" home_paths "$tmp/plain"

finished
