#!/usr/bin/env bash
# Maintainer-only, unpublished complete installer retention on a disposable runner.
set -euo pipefail
test "$GITHUB_ACTIONS" = true
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$GITHUB_RUN_ID" =~ ^[0-9]+$ && "$GITHUB_RUN_ATTEMPT" =~ ^[0-9]+$ ]]
command -v gh jq python3 > /dev/null
report="$1"
work="$(mktemp -d "$RUNNER_TEMP/complete-candidate-XXXXXX")"
nonce="$(python3 -c 'import uuid; print(uuid.uuid4().hex)')"
tag="candidate-${GITHUB_SHA:0:12}-${GITHUB_RUN_ID}-$nonce"
created=false
cleanup() {
  status=$?
  trap - EXIT
  if [ "$status" -ne 0 ] && [ "$created" = true ]; then
    if gh release view "$tag" --repo "$GITHUB_REPOSITORY" --json isDraft --jq .isDraft | grep -qx true; then
      gh release delete "$tag" --repo "$GITHUB_REPOSITORY" --cleanup-tag --yes || status=1
    else
      printf '%s\n' 'Candidate cleanup requires maintainer inspection.' >&2
      status=1
    fi
  fi
  rm -rf -- "$work"
  exit "$status"
}
trap cleanup EXIT
python3 scripts/check_source.py --repository
python3 scripts/package_delivery.py plan "$report" > "$work/plan.json"
# A unique tag must be absent before this invocation can own its draft or cleanup.
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" > "$work/existing.json"
jq -e --arg tag "$tag" '[.[][] | select(.tag_name == $tag)] | length == 0' "$work/existing.json" > /dev/null
if gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$tag" > "$work/ref.json" 2> "$work/ref-error"; then
  printf '%s\n' 'Candidate tag already exists.' >&2
  exit 1
fi
grep -q '404' "$work/ref-error"
jq -n --arg tag "$tag" --arg source "$GITHUB_SHA" \
  '{tag_name:$tag,target_commitish:$source,draft:true,prerelease:true,name:("AI Phone complete candidate "+$source[0:12]),body:"Unpublished complete installer candidate for maintainer verification. Physical installation and redistribution review remain separate requirements."}' > "$work/request.json"
created=true
gh api --method POST "repos/$GITHUB_REPOSITORY/releases" --input "$work/request.json" > "$work/release.json"
release_id="$(jq -er '.id' "$work/release.json")"
jq -e --arg tag "$tag" '.draft == true and .published_at == null and .tag_name == $tag' "$work/release.json" > /dev/null
paths=()
while IFS= read -r relative; do paths+=("$PWD/dist/$relative"); done < <(jq -er '.files[].path' "$work/plan.json")
gh release upload "$tag" --repo "$GITHUB_REPOSITORY" "${paths[@]}"
gh api "repos/$GITHUB_REPOSITORY/releases/$release_id" > "$work/remote.json"
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases/$release_id/assets?per_page=100" > "$work/assets.json"
jq --slurpfile pages "$work/assets.json" '.assets = ($pages[0] | add)' "$work/remote.json" > "$work/complete.json"
python3 scripts/package_delivery.py verify-upload "$work/plan.json" --release "$work/complete.json"
printf 'Unpublished complete candidate retained: %s\n' "$tag"
