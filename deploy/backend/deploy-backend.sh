#!/usr/bin/env bash
# Builds and (re)deploys the leaderboard service. Needs sudo.
#
#   deploy/backend/deploy-backend.sh            test, build, install, restart
#   deploy/backend/deploy-backend.sh --rollback  back to the previous build
#
# Code goes to /opt/cube-leaderboard (root-owned, read-only for the
# service), data stays in /var/lib/cube-leaderboard (user cubelb).
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
app=/opt/cube-leaderboard
data=/var/lib/cube-leaderboard
unit=cube-leaderboard.service
port=8737

health() {
    for _ in $(seq 1 30); do
        if curl -fsS --max-time 2 "http://127.0.0.1:$port/api/health" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.5
    done
    return 1
}

restart_and_check() {
    sudo systemctl restart "$unit"
    if ! health; then
        sudo journalctl -u "$unit" -n 30 --no-pager >&2 || true
        return 1
    fi
}

if [ "${1-}" = "--rollback" ]; then
    [ -d "$app.prev" ] || { echo "no previous build in $app.prev" >&2; exit 1; }
    sudo rm -rf "$app.failed"
    sudo mv -T "$app" "$app.failed"
    sudo mv -T "$app.prev" "$app"
    restart_and_check
    echo "rolled back (the replaced build is in $app.failed)"
    exit 0
fi

(cd "$repo/backend" && gleam format --check src test && gleam test && gleam export erlang-shipment)

if ! id -u cubelb >/dev/null 2>&1; then
    sudo useradd --system --user-group --home-dir "$data" --no-create-home \
        --shell /usr/sbin/nologin cubelb
fi
sudo install -d -o cubelb -g cubelb -m 0750 "$data"

stage=$(sudo mktemp -d /opt/.cube-leaderboard.XXXXXX)
sudo cp -a "$repo/backend/build/erlang-shipment/." "$stage/"
sudo chown -R root:root "$stage"
sudo chmod -R u=rwX,go=rX "$stage"
sudo chmod 0755 "$stage"

sudo install -m 0644 "$repo/deploy/backend/cube-leaderboard.service" "/etc/systemd/system/$unit"
sudo install -m 0755 "$repo/deploy/backend/cube-lb-moderate" /usr/local/sbin/cube-lb-moderate
# Daily backup: local snapshot + encrypted copy on Google Drive.
sudo install -m 0755 "$repo/deploy/backend/cube-lb-backup" /usr/local/sbin/cube-lb-backup
sudo install -m 0644 "$repo/deploy/backend/cube-leaderboard-backup.service" /etc/systemd/system/
sudo install -m 0644 "$repo/deploy/backend/cube-leaderboard-backup.timer" /etc/systemd/system/
sudo install -d -o micu -g micu -m 0700 /srv/backups/cube-leaderboard
sudo systemctl daemon-reload
sudo systemctl enable --quiet "$unit"
sudo systemctl enable --quiet --now cube-leaderboard-backup.timer

if [ -d "$app" ]; then
    sudo rm -rf "$app.prev"
    sudo mv -T "$app" "$app.prev"
fi
sudo mv -T "$stage" "$app"

if ! restart_and_check; then
    echo "health check failed" >&2
    if [ -d "$app.prev" ]; then
        echo "restoring the previous build" >&2
        sudo rm -rf "$app.failed"
        sudo mv -T "$app" "$app.failed"
        sudo mv -T "$app.prev" "$app"
        restart_and_check || true
    fi
    exit 1
fi
echo "cube-leaderboard deployed: $(curl -fsS "http://127.0.0.1:$port/api/health")"
