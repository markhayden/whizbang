#!/usr/bin/env bash
# Attribution by author, committer and tagger; tag messages.
source "$(dirname "$0")/common.sh"

# Who made the commit. A name counts where it starts a word, whatever is joined to it.
by() { # <author name> <author email> <committer name>
  echo "$1 $2 $3" >thing.txt
  git add thing.txt
  GIT_AUTHOR_NAME=$1 GIT_AUTHOR_EMAIL=$2 GIT_COMMITTER_NAME=$3 git commit -q --no-verify -m "feat: a thing"
}
vendor="An""thropic"
maker="Open""AI"
coder="Cod""ex"
chat="Chat""GPT"
twins="Gem""ini"
pilot="cop""ilot"
dev="dev""in"
aid="ai""der"
family="Op""us"
verse="Son""net"
fresh agents
: >"$tmp/agent-commits"
while IFS='|' read -r an ae cn; do
  by "$an" "$ae" "$cn"
  git rev-parse HEAD >>"$tmp/agent-commits"
done <<LIST
$N|tester@example.com|A Tester
A Tester|tester@example.com|$N
A Tester|x@$vendor.com|A Tester
$N Code|tester@example.com|A Tester
${N}Code|tester@example.com|A Tester
${N}Bot|tester@example.com|A Tester
${chat}4|tester@example.com|A Tester
$maker$coder|tester@example.com|A Tester
Cursor Agent|cursoragent@example.com|A Tester
$dev-ai-integration[bot]|bot@example.com|A Tester
$aid (${chat#Chat}-4o)|tester@example.com|A Tester
$pilot-swe-agent[bot]|bot@example.com|GitHub
$twins Jones|jones@example.com|A Tester
$vendor $verse|tester@example.com|A Tester
LIST
expect_fail "agents as author or committer" "attribution check:" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
while read -r sha; do
  says "commit $sha: the author or committer is an agent or a model"
done <"$tmp/agent-commits"
fresh people-and-bots
while IFS='|' read -r an ae cn; do
  by "$an" "$ae" "$cn"
done <<LIST
dependabot[bot]|49699333+dependabot[bot]@users.noreply.github.com|GitHub
renovate[bot]|29139614+renovate[bot]@users.noreply.github.com|GitHub
github-actions[bot]|41898282+github-actions[bot]@users.noreply.github.com|GitHub
Sam Egptian|sam@example.com|A Tester
$family Jones|jones@example.com|A Tester
$verse Writer|poet@example.com|A Tester
LIST
expect_pass "people and plain bots as author" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD

# An annotated tag whose message holds a key, on a commit the remote already has.
git checkout -q main
git tag -a v-planted -m "release notes: $key" "$first"
expect_fail "a key in a tag message" "tag-message/" git push origin v-planted
quiet "$key"
not_on_remote refs/tags/v-planted
git tag -a v-credited -m "release

  $credit: $N <noreply@example.com>" "$first"
expect_fail "a credit line in a tag message" "$has_credit" git push origin v-credited
git tag -a v-signed -m "release

Signed-off-by: Mark via $N <m@example.com>" "$first"
expect_fail "a trailer naming an agent in a tag message" "$credits_agent" git push origin v-signed
GIT_COMMITTER_NAME="${N}Bot" git tag -a v-agent -m "release" "$first"
expect_fail "an agent as the tagger" "the tagger is an agent or a model" git push origin v-agent
