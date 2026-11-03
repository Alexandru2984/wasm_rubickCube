#!/usr/bin/env bash
# Build and deploy cube.micutu.com atomically.
#
#   deploy/deploy.sh            build, publish a new release, switch to it
#   deploy/deploy.sh --no-build publish the existing frontend/dist as is
#   deploy/deploy.sh --rollback switch back to the previous release
#
# Layout (nginx root = $SITE/current):
#   $SITE/current -> releases/current -> <stamp>-<sha>/
# Switching is a single rename(2) of the releases/current symlink, so a
# request never sees a half-copied release, and the old files stay on disk
# for pages that are still downloading them.
set -euo pipefail

SITE=${SITE:-/srv/canary/cube.micutu.com}
KEEP=${KEEP:-5}
REL="$SITE/releases"
repo="$(cd "$(dirname "$0")/.." && pwd)"

die() { echo "deploy: $*" >&2; exit 1; }

swap_to() {
    ln -sfn "$1" "$REL/.current.tmp"
    mv -T "$REL/.current.tmp" "$REL/current"
    echo "deploy: live release is now $1"
}

# One-time migration from a plain directory to the releases layout. Needs
# sudo because $SITE is root-owned; afterwards everything runs as the user.
migrate() {
    [ -L "$SITE/current" ] && return 0
    echo "deploy: migrating $SITE/current to the releases layout (sudo)"
    local legacy
    legacy="legacy-$(date -u -r "$SITE/current/index.html" +%Y%m%d 2>/dev/null || echo old)"
    sudo install -d -o "$(id -u)" -g "$(id -g)" -m 755 "$REL"
    cp -a "$SITE/current" "$REL/$legacy"
    ln -sfn "$legacy" "$REL/current"
    sudo ln -sfn releases/current "$SITE/.current.new"
    sudo mv "$SITE/current" "$SITE/.current.dir-old"
    sudo mv -T "$SITE/.current.new" "$SITE/current"
    sudo rm -rf "$SITE/.current.dir-old"
}

prune() {
    local live
    live="$(readlink "$REL/current")"
    # Newest first; keep $KEEP releases plus whatever is live.
    find "$REL" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %f\n' | sort -rn | awk '{print $2}' |
        tail -n +"$((KEEP + 1))" | while read -r old; do
            [ "$old" = "$live" ] && continue
            rm -rf "${REL:?}/$old"
            echo "deploy: pruned $old"
        done
}

migrate
exec 9>"$REL/.deploy.lock"
flock -n 9 || die "another deploy is running"

case "${1:-}" in
--rollback)
    live="$(readlink "$REL/current")"
    prev="$(find "$REL" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %f\n' | sort -rn | awk '{print $2}' |
        grep -vx "$live" | head -1)"
    [ -n "$prev" ] || die "no previous release"
    swap_to "$prev"
    exit 0
    ;;
--no-build) ;;
"") "$repo/frontend/build.sh" ;;
*) die "unknown option $1" ;;
esac

dist="${DIST:-$repo/frontend/dist}"
[ -f "$dist/index.html" ] || die "$dist/index.html missing — build first"
grep -q '__PRECACHE__' "$dist/service-worker.js" && die "dist was not post-processed (run frontend/build.sh)"

sha="$(git -C "$repo" rev-parse --short=12 HEAD)"
git -C "$repo" diff --quiet HEAD -- frontend || sha="$sha-dirty"
name="$(date -u +%Y%m%dT%H%M%SZ)-$sha"
stage="$REL/.staging-$name"

rm -rf "$stage"
cp -a "$dist" "$stage"
find "$stage" -type d -exec chmod 755 {} +
find "$stage" -type f -exec chmod 644 {} +
mv -T "$stage" "$REL/$name"
touch "$REL/$name" # cp -a kept dist's mtime; prune/rollback order by deploy time

prev="$(readlink "$REL/current" || true)"
swap_to "$name"
prune
[ -n "$prev" ] && echo "deploy: roll back with: deploy/deploy.sh --rollback   (previous: $prev)"
