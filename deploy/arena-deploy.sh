#!/usr/bin/env bash
# Installs a deploy bundle sent by GitHub Actions (.github/workflows/deploy.yml), on the server as
# /usr/local/bin/arena-deploy. It's the only thing the CI's SSH key may run (a forced command in
# root's authorized_keys, set up by `bash deploy/deploy.sh ci-setup`), so that key can't open a
# shell. The bundle comes on stdin, a .tar.gz of:
#   bin/arena-server   the game server, built by the workflow
#   web/               the web client
# The commit it was built from is the SSH command (SSH_ORIGINAL_COMMAND).
#
# Nothing in the bundle runs as root: the server runs as the `arena` user, the web client is
# served by Caddy.
set -euo pipefail

APP_DIR=/opt/arena

commit="${SSH_ORIGINAL_COMMAND:-}"
[[ "$commit" =~ ^[0-9a-f]{7,40}$ ]] || { echo "Expected a commit hash as the command, got '$commit'."; exit 1; }

tmp="$(mktemp -d "$APP_DIR/deploy.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
tar --no-same-owner --no-same-permissions -xzf - -C "$tmp"

[ -f "$tmp/bin/arena-server" ] && [ -f "$tmp/web/index.html" ] || { echo "Bundle is missing bin/arena-server or web/index.html."; exit 1; }
# Plain files and directories only: a link could point Caddy at anything on the server.
if [ -n "$(find "$tmp" ! -type f ! -type d -print -quit)" ]; then
  echo "Bundle has something other than files and directories."
  exit 1
fi

install -m 755 "$tmp/bin/arena-server" "$APP_DIR/bin/arena-server.new"
mv "$APP_DIR/bin/arena-server.new" "$APP_DIR/bin/arena-server"
chmod -R u=rwX,go=rX "$tmp/web"
rm -rf "$APP_DIR/web.old"
[ -d "$APP_DIR/web" ] && mv "$APP_DIR/web" "$APP_DIR/web.old"
mv "$tmp/web" "$APP_DIR/web"
echo "$commit" > "$APP_DIR/deployed-commit"
/usr/local/bin/arena-cert-sync
systemctl restart arena
sleep 3
systemctl is-active arena
journalctl -u arena -n 5 --no-pager
echo "Deployed $commit."
