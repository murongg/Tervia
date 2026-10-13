#!/bin/sh
set -eu

fail() {
  printf 'Release stopped: %s\n' "$1" >&2
  exit 1
}

release_mode=local
for release_argument in "$@"; do
  case "$release_argument" in
    --dry-run|--push)
      [ "$release_mode" = local ] || fail 'Use either --dry-run or --push.'
      release_mode=$release_argument
      ;;
    -h|--help)
      printf '%s\n' 'Usage: sh scripts/release.sh [--dry-run | --push]' \
        'Read the committed Cargo version, generate CHANGELOG.md, commit, and create an annotated tag.' \
        '--dry-run  Preview without changing repository files or Git refs.' \
        '--push     Fetch origin tags, then atomically push the current branch and release tag.'
      exit 0
      ;;
    *) fail "Unknown argument: $release_argument" ;;
  esac
done

release_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$release_root"
release_tmp=
cleanup() {
  release_status=$?
  trap - 0
  if [ -n "$release_tmp" ]; then rm -rf -- "$release_tmp"; fi
  if [ "$release_status" -ne 0 ]; then
    printf '%s\n' 'Inspect git status; generated files, commits, and tags are preserved on failure.' >&2
  fi
  exit "$release_status"
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM

[ -z "$(git status --porcelain --untracked-files=all)" ] || fail 'Release requires a clean working tree; commit your changes first.'
release_branch=$(git branch --show-current)
[ -n "$release_branch" ] || fail 'Check out a branch before releasing; detached HEAD is not supported.'
[ "$(git rev-parse --is-shallow-repository)" = false ] || fail 'Fetch full history and tags before releasing (git fetch --unshallow --tags).'

# pkgid alone does not detect a stale root-package version in Cargo.lock.
if ! cargo metadata --locked --offline --format-version=1 >/dev/null; then
  fail 'Cannot validate Cargo.lock and cached dependencies; run cargo check and commit the version files first.'
fi
release_package=$(cargo pkgid --locked --offline)
release_version=${release_package##*#}
release_version=${release_version##*@}
release_tag=v$release_version
git check-ref-format "refs/tags/$release_tag"
if [ "$release_mode" = --push ]; then
  git remote get-url origin >/dev/null
  git fetch --tags origin
fi
[ -z "$(git tag --list "$release_tag")" ] || fail "Tag $release_tag already exists; update and commit the Cargo version first."

release_revision=HEAD
if [ -n "$(git tag --merged HEAD --list 'v[0-9]*')" ]; then
  release_previous=$(git describe --tags --match 'v[0-9]*' --abbrev=0)
  release_revision=$release_previous..HEAD
fi
release_tmp=$(mktemp -d "${TMPDIR:-/tmp}/tervia-release.XXXXXX")
git log --no-merges --format=%H "$release_revision" > "$release_tmp/commits"
while IFS= read -r release_commit; do
  git show -s --format=%s%n%b "$release_commit" > "$release_tmp/message"
  awk -v directory="$release_tmp" -v commit="$release_commit" '
    BEGIN { category = "Other changes" }
    NR == 1 {
      if ($0 ~ /^chore\(release\):/) { skip = 1; exit }
      description = $0
      if ($0 ~ /^[a-z]+(\([^)]+\))?!?: .+/) {
        prefix = substr($0, 1, index($0, ": ") - 1)
        kind = prefix
        sub(/[(!].*$/, "", kind)
        description = substr($0, index($0, ": ") + 2)
        if (kind == "feat") category = "Features"
        if (kind == "fix") category = "Fixes"
        if (kind == "perf") category = "Performance"
        if (kind == "docs") category = "Documentation"
        if (prefix ~ /!$/) category = "Breaking changes"
        if (prefix ~ /\(/) {
          scope = prefix
          sub(/^[^(]*\(/, "", scope)
          sub(/\)!?$/, "", scope)
          description = "**" scope ":** " description
        }
      }
      next
    }
    !breaking && /^BREAKING[ -]CHANGE:[[:space:]]*[^[:space:]]/ {
      breaking = $0
      sub(/^BREAKING[ -]CHANGE:[[:space:]]*/, "", breaking)
      category = "Breaking changes"
      description = description " — " breaking
    }
    END {
      if (!skip) print "- " description " (" substr(commit, 1, 8) ")" >> (directory "/" category)
    }
  ' "$release_tmp/message"
done < "$release_tmp/commits"

printf '## %s (%s)\n' "$release_tag" "$(date +%Y-%m-%d)" > "$release_tmp/notes"
release_has_entries=false
for release_category in 'Breaking changes' Features Fixes Performance Documentation 'Other changes'; do
  if [ -s "$release_tmp/$release_category" ]; then
    release_has_entries=true
    printf '\n### %s\n\n' "$release_category" >> "$release_tmp/notes"
    cat "$release_tmp/$release_category" >> "$release_tmp/notes"
  fi
done
[ "$release_has_entries" = true ] || fail 'No commits to release since the previous version tag.'

[ ! -L CHANGELOG.md ] || fail 'CHANGELOG.md must be a regular file, not a symlink.'
release_first_heading=
if [ -e CHANGELOG.md ]; then
  [ -f CHANGELOG.md ] || fail 'CHANGELOG.md must be a regular file.'
  [ "$(head -n 1 CHANGELOG.md)" = '# Changelog' ] || fail "CHANGELOG.md must start with '# Changelog'; its contents were preserved."
  if awk -v tag="$release_tag" '$1 == "##" && $2 == tag { found = 1 } END { exit !found }' CHANGELOG.md; then
    fail "CHANGELOG.md already contains $release_tag."
  fi
  # Copy existing entries verbatim, including their original Markdown formatting.
  release_first_heading=$(awk '/^## / { print NR; exit }' CHANGELOG.md)
  if [ -n "$release_first_heading" ]; then
    head -n "$((release_first_heading - 1))" CHANGELOG.md > "$release_tmp/changelog"
  else
    cat CHANGELOG.md > "$release_tmp/changelog"
    printf '\n' >> "$release_tmp/changelog"
  fi
else
  printf '# Changelog\n\n' > "$release_tmp/changelog"
fi
cat "$release_tmp/notes" >> "$release_tmp/changelog"
if [ -n "$release_first_heading" ]; then
  printf '\n' >> "$release_tmp/changelog"
  tail -n "+$release_first_heading" CHANGELOG.md >> "$release_tmp/changelog"
fi

if [ "$release_mode" = --dry-run ]; then
  cat "$release_tmp/notes"
  printf '\nWould commit CHANGELOG.md and create annotated tag %s on %s.\n' "$release_tag" "$release_branch"
  exit 0
fi

git var GIT_AUTHOR_IDENT >/dev/null
git var GIT_COMMITTER_IDENT >/dev/null
cat "$release_tmp/changelog" > CHANGELOG.md
git add -- CHANGELOG.md
git commit -m "chore(release): $release_tag"
# Hooks may edit files; tag notes must describe the changelog actually committed.
git show HEAD:CHANGELOG.md > "$release_tmp/committed"
if [ -n "$(git status --porcelain --untracked-files=all)" ] || ! cmp -s "$release_tmp/changelog" "$release_tmp/committed"; then
  fail 'A commit hook changed release files; inspect the release commit before tagging.'
fi
# Default tag cleanup removes Markdown headings as comments.
git tag -a "$release_tag" --cleanup=verbatim -F "$release_tmp/notes"

quote_argument() {
  printf "'"
  printf '%s' "$1" | sed "s/'/'\\\\''/g"
  printf "'"
}
print_push() {
  printf 'git push --atomic origin '
  quote_argument "HEAD:refs/heads/$release_branch"
  printf ' '
  quote_argument "refs/tags/$release_tag"
  printf '\n'
}
if [ "$release_mode" = --push ]; then
  # Do not publish the tag if the branch push is rejected, or vice versa.
  if ! git push --atomic origin "HEAD:refs/heads/$release_branch" "refs/tags/$release_tag"; then
    printf '%s\n' 'Local release commit and tag were preserved. After fixing the remote error, retry:' >&2
    print_push >&2
    exit 1
  fi
  printf 'Published %s and %s to origin; GitHub Actions will build the release.\n' "$release_tag" "$release_branch"
else
  printf 'Created CHANGELOG.md, a release commit, and annotated tag %s.\nTo publish:\n' "$release_tag"
  print_push
fi
