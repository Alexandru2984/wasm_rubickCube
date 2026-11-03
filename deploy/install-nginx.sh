#!/usr/bin/env bash
# Installs the vhost, header snippet and http-context limits, validates,
# reloads; restores the previous files if `nginx -t` fails. Needs sudo.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
site=/etc/nginx/sites-available/cube.micutu.com-canary
snippet=/etc/nginx/snippets/cube-headers.conf
limits=/etc/nginx/conf.d/cube-limits.conf
backup_dir=/etc/nginx/backups-cube
stamp=$(date -u +%Y%m%dT%H%M%SZ)

sudo install -d -m 755 "$backup_dir"
[ -f "$site" ] && sudo cp -a "$site" "$backup_dir/site.$stamp"
[ -f "$snippet" ] && sudo cp -a "$snippet" "$backup_dir/snippet.$stamp"
[ -f "$limits" ] && sudo cp -a "$limits" "$backup_dir/limits.$stamp"

sudo install -m 644 "$here/nginx/cube-headers.conf" "$snippet"
sudo install -m 644 "$here/nginx/cube-limits.conf" "$limits"
sudo install -m 644 "$here/nginx/cube.micutu.com.conf" "$site"

if ! sudo nginx -t; then
    echo "nginx -t failed — restoring previous config" >&2
    [ -f "$backup_dir/site.$stamp" ] && sudo cp -a "$backup_dir/site.$stamp" "$site"
    if [ -f "$backup_dir/snippet.$stamp" ]; then sudo cp -a "$backup_dir/snippet.$stamp" "$snippet"; else sudo rm -f "$snippet"; fi
    if [ -f "$backup_dir/limits.$stamp" ]; then sudo cp -a "$backup_dir/limits.$stamp" "$limits"; else sudo rm -f "$limits"; fi
    exit 1
fi
sudo systemctl reload nginx
echo "nginx reloaded (backups in $backup_dir, stamp $stamp)"
