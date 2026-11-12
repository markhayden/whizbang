# What every part of the scan tests starts from. Sourced, not run.
# Every case runs in a throwaway repository with a throwaway bare remote and a made-up word
# list. Keys, addresses, names and attribution lines are put together at run time, so these
# files hold nothing the real scan would flag.
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

scan="$root/scripts/scan.sh"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

chars=abcdefghij0123456789ABCDEFGHIJ
hex=0123456789abcdef0123456789abcdef
key="sk""-$chars"
home_path="/Us""ers/someone/notes"
address="someone@""corp.test"
word=zorblaxquint
N="Cl""aude"
credit="Co""-Authored-By"
zeros=0000000000000000000000000000000000000000

words="$tmp/words"
printf '%s\n\nflimwozzle\n' "$word" >"$words"
export WHIZBANG_WORDLIST="$words"

# A repository whose `main` is published to a bare remote, with this repo's hooks and scan
# installed (ignored there, as untracked tooling), so `git push` runs the real pre-push hook.
remote="$tmp/remote.git"
repo="$tmp/repo"
git init -q --bare -b main "$remote"
git init -q -b main "$repo"
cd "$repo"
git config user.name "A Tester"
git config user.email "tester@example.com"
mkdir -p scripts/hooks
cp "$scan" scripts/scan.sh
cp "$root/scripts/hooks/pre-push" "$root/scripts/hooks/commit-msg" scripts/hooks/
printf 'scripts/\n.env\n' >.git/info/exclude
git config core.hooksPath scripts/hooks
git remote add origin "$remote"
echo "hello" >readme.txt
git add readme.txt
git commit -q -m "docs: a readme"
git push -q origin main >/dev/null 2>&1
first=$(git rev-parse HEAD)

# commit_file <message> <file> <content>: one commit that sets the file's content. It is
# made with the commit hooks bypassed, as a commit from another machine would arrive.
commit_file() {
  mkdir -p "$(dirname "$2")"
  printf '%s\n' "$3" >"$2"
  git add -- "$2"
  git commit -q --no-verify -m "$1"
}
# commit_all <message>: one commit of everything in the working tree, hooks bypassed.
commit_all() {
  git add -A
  git commit -q --no-verify -m "$1"
}
# fresh <name>: a new branch from the published commit.
fresh() {
  git checkout -q -f -B "$1" "$first"
  git clean -q -f -d
}
# not_on_remote <ref>: the push was refused, so the remote does not have the ref.
not_on_remote() {
  [ -z "$(git ls-remote "$remote" "$1")" ] || fail "$1 reached the remote"
}
# quiet <text>: the last output does not hold the text.
quiet() {
  if grep -a -q -i -F -e "$1" "$tmp/out"; then
    cat "$tmp/out" >&2
    fail "the output holds text it must never print"
  fi
}
# says <text>: the last output holds the text.
says() {
  grep -a -q -F -e "$1" "$tmp/out" || {
    cat "$tmp/out" >&2
    fail "expected the output to say: $1"
  }
}
# tree_says <place>: `says`, for a scan of the working tree. When the place is missing it
# first reports what the folder held and what two more scans find, the second with git grep
# on one thread: FLAKE-NOTE
tree_says() {
  local f
  if ! grep -a -q -F -e "$1" "$tmp/out"; then
    cp "$tmp/out" "$tmp/first"
    {
      echo "missing from the working-tree scan: $1"
      git --version
      echo "the files of the folder, with their sizes in bytes:"
      for f in ./*; do [ -L "$f" ] || [ ! -f "$f" ] || wc -c "$f"; done
      echo "files git lists: $(git ls-files -co --exclude-standard | wc -l | tr -d ' ')"
      # The scan exits 1 when it finds anything, so its output is kept and then read.
      "$scan" >"$tmp/second" 2>&1 || true
      if grep -a -q -F -e "$1" "$tmp/second"; then
        echo "a second scan reports it"
      else
        echo "a second scan does not report it either"
      fi
      GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=grep.threads GIT_CONFIG_VALUE_0=1 "$scan" >"$tmp/second" 2>&1 || true
      if grep -a -q -F -e "$1" "$tmp/second"; then
        echo "a scan on one thread reports it"
      else
        echo "a scan on one thread does not report it either"
      fi
    } >&2
    cp "$tmp/first" "$tmp/out"
  fi
  says "$1"
}

has_credit="the message has a ${credit} line; this repo allows none, for a person or an agent"
credits_agent="the message credits an agent or a model"
