#!/usr/bin/env bash
# One-time setup of a fresh Ubuntu 24.04+ VPS (e.g. Hetzner Cloud) for the game. Run as root on
# the server with DOMAIN set; `bash deploy/deploy.sh setup` from your PC uploads and runs it for
# you. Point the domain's A record at this server first. Safe to re-run.
#
# What it does:
#   - build tools and Rust (the server is built here, from the source deploy.sh uploads), 2 GB
#     swap (building Bevy needs the memory), automatic security updates
#   - an `arena` user; source and build cache in /opt/arena, the server binary in /opt/arena/bin,
#     the web client in /opt/arena/web
#   - Caddy serves the web client on https://DOMAIN and gets (and renews) its certificate from
#     Let's Encrypt. The game server's WebTransport needs a certificate too, the same one: a copy
#     it can read goes to /var/lib/arena/tls whenever Caddy gets a new one, and the server
#     restarts itself to load it
#   - systemd service `arena` (from arena.service next to this script), logs in the journal
#   - firewall: SSH, 80 and 443 TCP (the page), 5888 UDP (the game)
#   - SSH: key login only (if root already has a key)
set -euo pipefail

APP_USER=arena
APP_DIR=/opt/arena
STATE_DIR=/var/lib/arena
GAME_PORT=5888
DOMAIN="${DOMAIN:-}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# Where Caddy (the Debian package) keeps the certificate; Let's Encrypt only (see the Caddyfile).
CADDY_CERTS=/var/lib/caddy/.local/share/caddy/certificates/acme-v02.api.letsencrypt.org-directory

[ "$(id -u)" -eq 0 ] || { echo "Run as root."; exit 1; }
[ -n "$DOMAIN" ] || { echo "Set DOMAIN (e.g. DOMAIN=arena.example.com)."; exit 1; }
export DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a

step() { printf '\n=== %s\n' "$*"; }

step "System packages"
apt-get update -q
apt-get upgrade -yq
apt-get install -yq curl ca-certificates ufw unattended-upgrades build-essential pkg-config caddy

step "Swap (2 GB)"
if ! swapon --show | grep -q .; then
  fallocate -l 2G /swapfile
  chmod 600 /swapfile
  mkswap /swapfile
  swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  echo 'vm.swappiness=10' > /etc/sysctl.d/99-swappiness.conf
  sysctl -q -p /etc/sysctl.d/99-swappiness.conf
else
  echo "Swap already on."
fi

step "Rust"
if [ ! -x /root/.cargo/bin/cargo ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
/root/.cargo/bin/rustup update stable
/root/.cargo/bin/cargo --version

step "App user and directories"
id "$APP_USER" >/dev/null 2>&1 || useradd --system --home-dir "$STATE_DIR" --shell /usr/sbin/nologin "$APP_USER"
mkdir -p "$APP_DIR/bin" "$APP_DIR/web" "$STATE_DIR/tls"
chown -R "$APP_USER:$APP_USER" "$STATE_DIR"
chmod 750 "$STATE_DIR/tls"

step "Firewall"
ufw default deny incoming
ufw default allow outgoing
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow "$GAME_PORT/udp"
ufw --force enable
ufw status

step "SSH: key login only"
if [ -s /root/.ssh/authorized_keys ]; then
  # Sorts before Ubuntu's 50-cloud-init.conf, and sshd keeps the first value it reads.
  cat > /etc/ssh/sshd_config.d/10-hardening.conf <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
EOF
  mkdir -p /run/sshd # Needed by the config check; socket-activated sshd may not have made it yet.
  sshd -t
  systemctl try-reload-or-restart ssh
  echo "Password login disabled."
else
  echo "No key in /root/.ssh/authorized_keys, leaving password login on. Add a key and re-run."
fi

step "HTTPS on $DOMAIN (Caddy)"
cat > /etc/caddy/Caddyfile <<EOF
# Managed by setup-server.sh. Caddy gets and renews the certificate on its own.
{
	# Let's Encrypt only, so the certificate is always in the same place: the game server runs
	# on a copy of it (arena-cert-sync).
	acme_ca https://acme-v02.api.letsencrypt.org/directory
}

$DOMAIN {
	root * $APP_DIR/web
	file_server
	encode zstd gzip
	# The client's file names don't change between deploys: always check for a newer one.
	header Cache-Control "no-cache"
}
EOF
systemctl enable caddy
systemctl reload-or-restart caddy

step "Certificate copy for the game server"
cat > /usr/local/bin/arena-cert-sync <<EOF
#!/bin/sh
# Copies Caddy's certificate for $DOMAIN where the game server (user $APP_USER) can read it. The
# server notices the new certificate and restarts itself to load it.
set -e
src="$CADDY_CERTS/$DOMAIN"
[ -f "\$src/$DOMAIN.crt" ] && [ -f "\$src/$DOMAIN.key" ] || exit 0
install -o $APP_USER -g $APP_USER -m 600 "\$src/$DOMAIN.key" "$STATE_DIR/tls/key.pem.new"
install -o $APP_USER -g $APP_USER -m 644 "\$src/$DOMAIN.crt" "$STATE_DIR/tls/cert.pem.new"
# The key first: the server goes by the certificate file.
mv "$STATE_DIR/tls/key.pem.new" "$STATE_DIR/tls/key.pem"
mv "$STATE_DIR/tls/cert.pem.new" "$STATE_DIR/tls/cert.pem"
EOF
chmod 755 /usr/local/bin/arena-cert-sync
cat > /etc/systemd/system/arena-cert-sync.service <<EOF
[Unit]
Description=Copy Caddy's certificate for the arena game server

[Service]
Type=oneshot
ExecStart=/usr/local/bin/arena-cert-sync
EOF
# When Caddy writes a new certificate; and daily, in case a change was missed.
cat > /etc/systemd/system/arena-cert-sync.path <<EOF
[Unit]
Description=Watch Caddy's certificate for the arena game server

[Path]
PathChanged=$CADDY_CERTS/$DOMAIN/$DOMAIN.crt

[Install]
WantedBy=multi-user.target
EOF
cat > /etc/systemd/system/arena-cert-sync.timer <<EOF
[Unit]
Description=Daily copy of Caddy's certificate for the arena game server

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now arena-cert-sync.path arena-cert-sync.timer
/usr/local/bin/arena-cert-sync

step "systemd service"
cp "$SCRIPT_DIR/arena.service" /etc/systemd/system/arena.service
systemctl daemon-reload
systemctl enable arena
if [ -x "$APP_DIR/bin/arena-server" ]; then
  systemctl restart arena
fi

step "Done"
echo "Page:  https://$DOMAIN  (once DNS points here and Caddy has the certificate)"
echo "Game:  UDP $GAME_PORT (WebTransport, same certificate)"
echo "Logs:  journalctl -u arena -f   (or from your PC: bash deploy/deploy.sh logs)"
echo "If your provider has its own firewall (e.g. Hetzner Cloud Firewall), open the same ports there."
[ -x "$APP_DIR/bin/arena-server" ] || echo "Next: deploy from your PC with: bash deploy/deploy.sh app"
