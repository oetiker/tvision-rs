#!/bin/bash
# Manage versioned documentation for GitHub Pages.
# Usage: manage-doc-versions.sh <site-dir> <action> [version]
#   <site-dir> is the project sub-root inside the Pages tree, e.g. site/rstv.
#   Actions:
#     cull               - Remove old versions (keep latest patch of last N minors)
#     update-json        - Regenerate versions.json from the directories present
#     generate-redirect  - Generate root index.html redirect to latest stable
#
# The site layout this manages:
#   <site-dir>/dev/         rolling docs built from main
#   <site-dir>/vX.Y.Z/      one directory per released version
#   <site-dir>/versions.json
#   <site-dir>/index.html   redirect to latest stable (or dev if none)

set -euo pipefail

SITE_DIR="${1:?Site directory required}"
ACTION="${2:?Action required}"
VERSION="${3:-}"

# URL prefix the site is served under (GitHub Pages project site path).
URL_BASE="/rstv"

VERSIONS_JSON="$SITE_DIR/versions.json"
KEEP_MINOR_VERSIONS=4

init_versions_json() {
    if [[ ! -f "$VERSIONS_JSON" ]]; then
        echo '{"latest": null, "versions": []}' > "$VERSIONS_JSON"
    fi
}

# All version directories (vX.Y.Z), sorted ascending. Excludes dev.
get_version_dirs() {
    find "$SITE_DIR" -maxdepth 1 -type d -name 'v*' 2>/dev/null | \
        xargs -I{} basename {} | \
        sort -V
}

# v0.8.1 -> "0.8"
get_minor() {
    echo "$1" | sed 's/^v//' | cut -d. -f1,2
}

# Keep only the latest patch of each minor, then only the last N minors.
cull_versions() {
    local versions
    versions=$(get_version_dirs)

    if [[ -z "$versions" ]]; then
        echo "No versions to cull"
        return
    fi

    declare -A minor_to_latest

    for v in $versions; do
        minor=$(get_minor "$v")
        current_latest="${minor_to_latest[$minor]:-}"

        if [[ -z "$current_latest" ]]; then
            minor_to_latest[$minor]="$v"
        else
            current_patch=$(echo "$current_latest" | sed 's/^v//' | cut -d. -f3)
            new_patch=$(echo "$v" | sed 's/^v//' | cut -d. -f3)
            if [[ "$new_patch" -gt "$current_patch" ]]; then
                echo "Removing older patch: $current_latest (keeping $v)"
                rm -rf "${SITE_DIR:?}/$current_latest"
                minor_to_latest[$minor]="$v"
            else
                echo "Removing older patch: $v (keeping $current_latest)"
                rm -rf "${SITE_DIR:?}/$v"
            fi
        fi
    done

    local kept_minors
    kept_minors=$(printf '%s\n' "${!minor_to_latest[@]}" | sort -V | tail -n "$KEEP_MINOR_VERSIONS")

    for minor in "${!minor_to_latest[@]}"; do
        if ! echo "$kept_minors" | grep -q "^${minor}$"; then
            local v="${minor_to_latest[$minor]}"
            echo "Removing old minor version: $v"
            rm -rf "${SITE_DIR:?}/$v"
        fi
    done
}

update_versions_json() {
    init_versions_json

    local versions
    versions=$(get_version_dirs | sort -Vr)
    local latest=""

    if [[ -n "$versions" ]]; then
        latest=$(echo "$versions" | head -1)
    fi

    local json_versions="["
    local first=true

    if [[ -d "$SITE_DIR/dev" ]]; then
        json_versions+="{\"version\":\"dev\",\"path\":\"${URL_BASE}/dev/\",\"prerelease\":true}"
        first=false
    fi

    for v in $versions; do
        if [[ "$first" == "false" ]]; then
            json_versions+=","
        fi
        json_versions+="{\"version\":\"$v\",\"path\":\"${URL_BASE}/$v/\"}"
        first=false
    done

    json_versions+="]"

    cat > "$VERSIONS_JSON" << EOF
{
  "latest": "${latest:-null}",
  "versions": $json_versions
}
EOF

    echo "Updated versions.json:"
    cat "$VERSIONS_JSON"
}

generate_redirect() {
    init_versions_json

    local latest
    latest=$(grep -o '"latest": *"[^"]*"' "$VERSIONS_JSON" | cut -d'"' -f4 || true)

    if [[ -z "$latest" || "$latest" == "null" ]]; then
        # No stable version yet, redirect to dev.
        latest="dev"
    fi

    cat > "$SITE_DIR/index.html" << EOF
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>rstv Documentation</title>
    <script>
        // Redirect to the latest stable version (falls back to dev).
        fetch('${URL_BASE}/versions.json')
            .then(function (r) { return r.json(); })
            .then(function (data) {
                var target = data.latest ? ('${URL_BASE}/' + data.latest + '/') : '${URL_BASE}/dev/';
                window.location.replace(target);
            })
            .catch(function () {
                window.location.replace('${URL_BASE}/${latest}/');
            });
    </script>
    <noscript>
        <meta http-equiv="refresh" content="0; url=${URL_BASE}/${latest}/">
    </noscript>
</head>
<body>
    <p>Redirecting to documentation&hellip;</p>
</body>
</html>
EOF

    echo "Generated redirect to $latest"
}

case "$ACTION" in
    cull)              cull_versions ;;
    update-json)       update_versions_json ;;
    generate-redirect) generate_redirect ;;
    *) echo "Unknown action: $ACTION"; exit 1 ;;
esac
