#!/usr/bin/env bash
# Maintainer-only, unpublished build output retention on a disposable runner.
set -euo pipefail
test "$GITHUB_ACTIONS" = true
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$GITHUB_RUN_ID" =~ ^[0-9]+$ && "$GITHUB_RUN_ATTEMPT" =~ ^[0-9]+$ ]]
command -v gh jq python3 > /dev/null
report="$1"
case "${2:-client}" in
  client)
    planner=scripts/package_delivery.py
    label='AI Phone complete candidate'
    description='Unpublished complete installer candidate for maintainer verification. Physical installation and redistribution review remain separate requirements.'
    ;;
  source-emulator)
    planner=scripts/source_emulator_candidate.py
    label='AI Phone source-emulator candidate'
    description='Unpublished source-emulator output for maintainer review. Redistribution, source closure and Android boot qualification remain unapproved.'
    ;;
  factory-images)
    planner=scripts/factory_image_candidate.py
    label='AI Phone native factory images'
    description='Temporary unpublished ARM64 image handoff for complete native client assembly. No user state or publication approval is included.'
    ;;
  *) printf '%s\n' 'Unknown build candidate kind.' >&2; exit 1 ;;
esac
work="$(mktemp -d "$RUNNER_TEMP/build-candidate-XXXXXX")"
nonce="$(python3 -c 'import sys,uuid; sys.stdout.write(uuid.uuid4().hex)')"
tag="candidate-${GITHUB_SHA:0:12}-${GITHUB_RUN_ID}-$nonce"
created=false
release_id=""
cleanup() {
  status=$?
  trap - EXIT
  if [ "$status" -ne 0 ] && [ "$created" = true ]; then
    if [ -n "$release_id" ] && gh api "repos/$GITHUB_REPOSITORY/releases/$release_id" > "$work/failed-release.json" && \
      jq -be --arg tag "$tag" --arg source "$GITHUB_SHA" '.draft == true and .published_at == null and .tag_name == $tag and .target_commitish == $source' "$work/failed-release.json" > /dev/null; then
      gh api --method DELETE "repos/$GITHUB_REPOSITORY/releases/$release_id" || status=1
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
python3 "$planner" plan "$report" > "$work/plan.json"
# A unique tag must be absent before this invocation can own its draft or cleanup.
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" > "$work/existing.json"
jq -be --arg tag "$tag" '[.[][] | select(.tag_name == $tag)] | length == 0' "$work/existing.json" > /dev/null
if gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$tag" > "$work/ref.json" 2> "$work/ref-error"; then
  printf '%s\n' 'Candidate tag already exists.' >&2
  exit 1
fi
grep -q '404' "$work/ref-error"
jq -bn --arg tag "$tag" --arg source "$GITHUB_SHA" --arg label "$label" --arg description "$description" \
  '{tag_name:$tag,target_commitish:$source,draft:true,prerelease:true,name:($label+" "+$source[0:12]),body:$description}' > "$work/request.json"
created=true
gh api --method POST "repos/$GITHUB_REPOSITORY/releases" --input "$work/request.json" > "$work/release.json"
release_id="$(jq -ber '.id' "$work/release.json")"
jq -be --arg tag "$tag" '.draft == true and .published_at == null and .tag_name == $tag' "$work/release.json" > /dev/null
paths=()
# Native jq.exe must emit LF in Git Bash; CR would become part of each filename.
while IFS= read -r relative; do paths+=("$PWD/dist/$relative"); done < <(jq -ber '.files[].path' "$work/plan.json")
gh release upload "$tag" --repo "$GITHUB_REPOSITORY" "${paths[@]}"
gh api "repos/$GITHUB_REPOSITORY/releases/$release_id" > "$work/remote.json"
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases/$release_id/assets?per_page=100" > "$work/assets.json"
jq -b --slurpfile pages "$work/assets.json" '.assets = ($pages[0] | add)' "$work/remote.json" > "$work/complete.json"
python3 scripts/package_delivery.py verify-upload "$work/plan.json" --release "$work/complete.json"
printf 'Unpublished candidate retained: %s\n' "$tag"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  printf 'candidate_tag=%s\n' "$tag" >> "$GITHUB_OUTPUT"
  printf 'candidate_release_id=%s\n' "$release_id" >> "$GITHUB_OUTPUT"
fi
