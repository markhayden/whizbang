#!/usr/bin/env bash
# The census's list of the working tree is exactly git's, on trees nobody built by hand:
# each seed makes a few trees side by side, every one with its own made-up ignore rules,
# files, folders, links and tracked files, from names the rules are made of too. One scan
# for each seed. (scripts/tests/scan/folder.sh holds the same on a tree built by hand.)
#
#   WHIZBANG_SCAN_SEEDS="1 2 3"   other seeds; WHIZBANG_SCAN_TREES=18 trees for each
#   WHIZBANG_SCAN_ACCENTS=1       names with an accent are tracked too, as the folder has them
#   WHIZBANG_SCAN_PRECOMPOSE=false  core.precomposeUnicode for the trees (macOS)
#   WHIZBANG_SCAN_TALLY=<file>    a seed that fails is noted there with its first line of
#                                 output, and the run goes on: for looking at many seeds
#
# The trees hold no repository inside a repository: the scan refuses those earlier. A
# seed gives the same trees on every system: nothing in the generator asks the file
# system what is there.
source "$(dirname "$0")/common.sh"
stand_in_for_git

cat >"$tmp/make-trees.pl" <<'PL'
use strict;
use warnings;
my ($seed, $trees, $accents) = @ARGV;
srand($seed);
my @plain = ("a", "b", "c", "d", "keep", "out", "build", "log", "x.log", "y.log", "z.tmp", "k.me", "a.b");
my @odd = ("sp ace", "-dash", "st*r", "q?m", "br[ab]k", "two\nlines", "caf\303\251", "cafe\314\201", "!bang", "#hash");
sub pick { $_[int(rand(@_))] }
sub name { rand() < 0.75 ? pick(@plain) : pick(@odd) }
# A name as a rule would write it: no newline, and the marks that start a rule escaped.
sub in_rule { my $n; do { $n = name() } while ($n =~ /\n/); $n =~ s/^([!#])/\\$1/; $n }
sub rule {
  my ($n, $m) = (in_rule(), in_rule());
  my @shapes = ($n, "$n/", "/$n", "!$n", "!$n/", "*.log", "*.tmp", "!*.log", "!x.log", "$n/*", "!$n/$m", "$n/**",
    "**/$n", "$n/$m", "!$n/$m/", "$n/**/$m", "!$n/**/$m", "*", "!*/", "/*", "!/$n", "$n*", "?", "[a-c]", "!$n/**");
  pick(@shapes);
}
sub rules {
  my ($file, $most) = @_;
  open(my $out, ">", $file) or die "$file: $!";
  print $out rule(), "\n" for 1 .. $most - int(rand(3));
  close($out);
}
my (@tracked, %used);
sub fill {
  my ($dir, $depth) = @_;
  rules("$dir/.gitignore", 3) if $depth > 0 && rand() < 0.2;
  for (1 .. 2 + int(rand(4))) {
    my $path = "$dir/" . name();
    # One name for each folder, whatever the file system takes for the same name: the
    # two forms of the accented name count as one, so a seed gives the same trees on
    # every system.
    (my $same = $path) =~ s/cafe\314\201/caf\303\251/g;
    next if $used{$same}++;
    my $kind = rand();
    if ($kind < 0.35 && $depth < 3) {
      mkdir($path) or die "$path: $!";
      fill($path, $depth + 1);
    } elsif ($kind < 0.40) {
      symlink(pick(".", "..", "a", "keep"), $path) or die "$path: $!";
    } else {
      open(my $out, ">", $path) or die "$path: $!";
      print $out "content\n" unless rand() < 0.05;
      close($out);
      # A name with an accent is tracked only when asked for: on macOS the index then
      # holds it as the folder does, and a rule in the other form refuses the scan
      # (docs/carry-over.md).
      my $track = rand() < 0.15;
      push @tracked, $path if $track && ($accents || $path !~ /[\200-\377]/);
    }
  }
}
# At the top, rules that cannot take every tree out at once.
open(my $top, ">", ".gitignore") or die ".gitignore: $!";
print $top pick("**/" . in_rule() . "/", "!" . in_rule(), "*.tmp", "**/" . in_rule() . "/*"), "\n" for 1 .. 3;
close($top);
for my $t (1 .. $trees) {
  mkdir("t$t") or die "t$t: $!";
  rules("t$t/.gitignore", 9);
  fill("t$t", 0);
}
push @tracked, ".gitignore";
print "$_\0" for @tracked;
PL

for seed in ${WHIZBANG_SCAN_SEEDS:-11 23}; do
  fresh "seed-$seed"
  # What the last seed's rules ignored, too.
  git clean -q -f -d -x -e /scripts
  [ -z "${WHIZBANG_SCAN_PRECOMPOSE:-}" ] || git config core.precomposeunicode "$WHIZBANG_SCAN_PRECOMPOSE"
  perl "$tmp/make-trees.pl" "$seed" "${WHIZBANG_SCAN_TREES:-18}" "${WHIZBANG_SCAN_ACCENTS:-0}" >"$tmp/tracked"
  git -c core.precomposeunicode=false --literal-pathspecs add -f --pathspec-from-file="$tmp/tracked" --pathspec-file-nul
  git commit -q --no-verify -m "feat: the trees of seed $seed"
  if [ -n "${WHIZBANG_SCAN_TALLY:-}" ]; then
    (lists_what_git_lists "the trees of seed $seed") >"$tmp/seed-out" 2>&1 ||
      printf '%s: %s\n' "$seed" "$(grep -a -m 1 -E -e '^(scan|failed|[<>0-9])' "$tmp/seed-out" | cut -c1-160)" >>"$WHIZBANG_SCAN_TALLY"
  else
    lists_what_git_lists "the trees of seed $seed"
  fi
done

finished
