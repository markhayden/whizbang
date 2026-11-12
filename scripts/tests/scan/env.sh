#!/usr/bin/env bash
# The values of a local .env.
source "$(dirname "$0")/common.sh"

# The values of a local .env: the variable is named, the value is never printed.
fresh env-tree
secret_value="value-$(printf '%s' 0918273645)-of-env"
{
  printf '# local\nexport SOME_TOKEN="%s"\nSHORT=abc\n' "$secret_value"
  printf 'TOKEN_A=%s-a # the token\n' "$secret_value"
  printf 'TOKEN_B = "%s-b" # quoted\n' "$secret_value"
  printf 'export   TOKEN_C=%s-c\n' "$secret_value"
  printf 'export\tTOKEN_D=%s-d\n' "$secret_value"
} >.env
printf 'x = %s\n' "$secret_value" >planted.txt
expect_fail "a value from .env in the working tree" "planted.txt:1 (the value of SOME_TOKEN in .env)" "$scan"
quiet "$secret_value"
printf '%s-a\n%s-b\n%s-c\n%s-d\n' "$secret_value" "$secret_value" "$secret_value" "$secret_value" >planted.txt
expect_fail ".env values with a comment, spaces and export" "planted.txt:1 (the value of TOKEN_A in .env)" "$scan"
says "planted.txt:2 (the value of TOKEN_B in .env)"
says "planted.txt:3 (the value of TOKEN_C in .env)"
says "planted.txt:4 (the value of TOKEN_D in .env)"
quiet "$secret_value"
printf 'abc is short\n' >planted.txt
expect_pass "a short .env value is not searched for" "$scan"
rm planted.txt
commit_file "feat: a setting" settings.txt "x = $secret_value"
expect_fail "a value from .env in a commit" "settings.txt:1 (the value of SOME_TOKEN in .env)" "$scan" --secrets-only --range "$first" HEAD
quiet "$secret_value"
rm .env

finished
