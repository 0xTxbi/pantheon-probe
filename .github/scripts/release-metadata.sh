#!/usr/bin/env bash

set -euo pipefail

if [[ -z "${GITHUB_OUTPUT:-}" ]]; then
  echo "GITHUB_OUTPUT is required" >&2
  exit 1
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
tag="$version"
repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
sha="${GITHUB_SHA:?GITHUB_SHA is required}"
release_path_pattern='^(src/|Cargo\.toml$|Cargo\.lock$)'

tag_exists=false
if git rev-parse "$tag" >/dev/null 2>&1 || git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
  tag_exists=true
fi

release_exists=false
release_name=""
release_body=""
if gh release view "$tag" --json name,body >/tmp/pantheon-probe-release.json 2>/dev/null; then
  release_exists=true
  release_name="$(jq -r '.name // ""' /tmp/pantheon-probe-release.json)"
  release_body="$(jq -r '.body // ""' /tmp/pantheon-probe-release.json)"
fi

release_names=()
while IFS= read -r release_name_entry; do
  release_names+=("$release_name_entry")
done < <(gh release list --limit 100 --json name --jq '.[].name' 2>/dev/null || true)

generate_title() {
  local -a adjectives=(
    signal atlas mystic celestial lucid ember cobalt solar tidal velvet static radiant
    lunar amber astral silent silver neon
  )
  local -a nouns=(
    ledger harbor echo cascade mariner beacon horizon current circuit compass relay
    lantern atlas vector skyline archive
  )
  local total seed index attempt adjective_index noun_index candidate

  total=$(( ${#adjectives[@]} * ${#nouns[@]} ))
  seed="$(printf '%s' "${version}-${sha}" | cksum | awk '{print $1}')"
  index=$(( seed % total ))

  for ((attempt = 0; attempt < total; attempt++)); do
    adjective_index=$(( (index + attempt) % ${#adjectives[@]} ))
    noun_index=$(( ((index + attempt) / ${#adjectives[@]}) % ${#nouns[@]} ))
    candidate="${adjectives[$adjective_index]} ${nouns[$noun_index]}"

    if [[ "$candidate" == "$release_name" ]]; then
      printf '%s\n' "$candidate"
      return
    fi

    if [[ ! " ${release_names[*]} " =~ " ${candidate} " ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf 'signal %s\n' "${version//./-}"
}

sanitize_pr_body() {
  sed $'s/\r$//'
}

latest_tag="$(
  git tag --list \
    | sort -V \
    | tail -n 1 || true
)"

previous_version=""
if [[ -n "$latest_tag" ]]; then
  previous_version="$(
    git show "${latest_tag}:Cargo.toml" 2>/dev/null \
      | sed -n 's/^version = "\(.*\)"/\1/p' \
      | head -n 1
  )"
fi

version_bumped=true
if [[ -n "$previous_version" && "$previous_version" == "$version" ]]; then
  version_bumped=false
fi

changed_files=()
if [[ -n "$latest_tag" ]]; then
  while IFS= read -r changed_file; do
    [[ -n "$changed_file" ]] && changed_files+=("$changed_file")
  done < <(git diff --name-only "${latest_tag}..${sha}")
else
  while IFS= read -r changed_file; do
    [[ -n "$changed_file" ]] && changed_files+=("$changed_file")
  done < <(git diff-tree --no-commit-id --name-only -r "$sha")
fi

has_releasable_changes=false
if [[ ${#changed_files[*]} -gt 0 ]]; then
  for changed_file in "${changed_files[@]}"; do
    if [[ "$changed_file" =~ $release_path_pattern ]]; then
      has_releasable_changes=true
      break
    fi
  done
fi

crate_published=false
crate_status="$(
  curl -fsS -o /tmp/pantheon-probe-crate.json -w "%{http_code}" \
    "https://crates.io/api/v1/crates/pantheon-probe/${version}" || true
)"
if [[ "$crate_status" == "200" ]]; then
  crate_published=true
fi

already_released=false
if [[ "$tag_exists" == true && "$release_exists" == true && "$crate_published" == true ]]; then
  already_released=true
fi

should_release=true
if [[ "$already_released" == true || "$version_bumped" == false || "$has_releasable_changes" == false ]]; then
  should_release=false
fi

pr_json="$(gh api -H "Accept: application/vnd.github+json" "repos/${repo}/commits/${sha}/pulls" 2>/dev/null || printf '[]')"
pr_title="$(jq -r '.[0].title // ""' <<<"$pr_json")"
pr_body="$(jq -r '.[0].body // ""' <<<"$pr_json" | sanitize_pr_body)"

title="$release_name"
notes="$release_body"

if [[ -z "${title//[[:space:]]/}" || "$title" == "$tag" ]]; then
  title="$(generate_title)"
fi

if [[ -z "${notes//[[:space:]]/}" ]]; then
  if [[ -n "${pr_body//[[:space:]]/}" ]]; then
    notes="$pr_body"
  else
    summary="PantheonProbe ${version} ships the latest merged release work."
    if [[ -n "${pr_title//[[:space:]]/}" ]]; then
      cleaned_title="$pr_title"
      if [[ "$cleaned_title" == *[.!?] ]]; then
        cleaned_title="${cleaned_title%?}"
      fi
      summary="PantheonProbe ${version} ships ${cleaned_title}."
    fi

    range="$sha"
    if [[ -n "$latest_tag" ]]; then
      range="${latest_tag}..${sha}"
    fi

    subjects=()
    while IFS= read -r subject; do
      subjects+=("$subject")
    done < <(git log --format='%s' "$range" | head -n 5)

    notes="$summary"
    if (( ${#subjects[@]} > 0 )); then
      notes+=$'\n\n'
      for subject in "${subjects[@]}"; do
        notes+="- ${subject}"$'\n'
      done
      notes="${notes%$'\n'}"
    fi
  fi
fi

{
  printf 'should_release=%s\n' "$should_release"
  printf 'version=%s\n' "$version"
  printf 'tag=%s\n' "$tag"
  printf 'title=%s\n' "$title"
  printf 'previous_tag=%s\n' "$latest_tag"
  printf 'previous_version=%s\n' "$previous_version"
  printf 'version_bumped=%s\n' "$version_bumped"
  printf 'has_releasable_changes=%s\n' "$has_releasable_changes"
  printf 'notes<<EOF\n%s\nEOF\n' "$notes"
} >>"$GITHUB_OUTPUT"
