#!/usr/bin/env bash
# Deploy to a VPS from your PC (Git Bash on Windows works; needs ssh, tar and git, plus what
# scripts/build-web.sh needs).
#
#   bash deploy/deploy.sh setup    one-time server setup (runs deploy/setup-server.sh on the server)
#   bash deploy/deploy.sh app      build the web client here and the server there, then restart it
#   bash deploy/deploy.sh logs     follow the game server's logs
#   bash deploy/deploy.sh ci-setup let GitHub Actions deploy pushes to `prod` (needs gh, logged in)
#
# Settings come from the environment or .env.local: DEPLOY_HOST (SSH target, e.g.
# root@157.180.80.31) and DEPLOY_DOMAIN (e.g. arena.example.com, its A record pointing there).
set -euo pipefail
cd "$(dirname "$0")/.."

APP_DIR=/opt/arena

case "${1:-}" in
  setup | app | logs | ci-setup) ;;
  *) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac

# Value of KEY from the environment, else from .env.local.
setting() {
  if [ -n "${!1:-}" ]; then echo "${!1}"; return; fi
  if [ -f .env.local ]; then sed -n "s/^$1=\(.*\)$/\1/p" .env.local | tr -d '\r"' | tail -n1; fi
}
DEPLOY_HOST="$(setting DEPLOY_HOST)"
DEPLOY_DOMAIN="$(setting DEPLOY_DOMAIN)"
[ -n "${DEPLOY_HOST:-}" ] || { echo "Set DEPLOY_HOST (e.g. root@157.180.80.31) in .env.local or the environment."; exit 1; }

# Upload a local file with Windows line endings stripped.
upload() { tr -d '\r' < "$1" | ssh "$DEPLOY_HOST" "cat > '$2'"; }

case "${1:-}" in
  setup)
    [ -n "${DEPLOY_DOMAIN:-}" ] || { echo "Set DEPLOY_DOMAIN (e.g. arena.example.com) in .env.local or the environment."; exit 1; }
    ssh "$DEPLOY_HOST" "mkdir -p /root/arena-setup"
    upload deploy/setup-server.sh /root/arena-setup/setup-server.sh
    upload deploy/arena.service /root/arena-setup/arena.service
    ssh -t "$DEPLOY_HOST" "DOMAIN='$DEPLOY_DOMAIN' bash /root/arena-setup/setup-server.sh"
    ;;

  app)
    # The server is built from the last commit, and the web client must speak the same protocol.
    if ! git diff --quiet HEAD; then
      echo "Uncommitted changes: commit them first (the server is built from the last commit, the web"
      echo "client from your working tree, and the two must match)."
      exit 1
    fi
    commit="$(git rev-parse --short HEAD)"
    ./scripts/build-web.sh
    echo "Uploading the web client and the source of $commit to $DEPLOY_HOST ..."
    tar -C client/web --exclude=digest.txt -czf - . | ssh "$DEPLOY_HOST" "set -e
      rm -rf $APP_DIR/web.new && mkdir $APP_DIR/web.new
      tar --no-same-owner -xzf - -C $APP_DIR/web.new"
    git archive --format=tar.gz HEAD | ssh "$DEPLOY_HOST" "set -e
      rm -rf $APP_DIR/src && mkdir $APP_DIR/src
      tar --no-same-owner -xzf - -C $APP_DIR/src"
    echo "Building the server on $DEPLOY_HOST (the first build takes a while) ..."
    ssh "$DEPLOY_HOST" "set -e
      . /root/.cargo/env
      cd $APP_DIR/src
      CARGO_TARGET_DIR=$APP_DIR/target cargo build --release --locked -p arena-server
      install -m 755 $APP_DIR/target/release/arena-server $APP_DIR/bin/arena-server.new
      mv $APP_DIR/bin/arena-server.new $APP_DIR/bin/arena-server
      rm -rf $APP_DIR/web.old
      [ -d $APP_DIR/web ] && mv $APP_DIR/web $APP_DIR/web.old
      mv $APP_DIR/web.new $APP_DIR/web
      echo $commit > $APP_DIR/deployed-commit
      /usr/local/bin/arena-cert-sync
      systemctl restart arena
      sleep 3
      systemctl is-active arena
      journalctl -u arena -n 5 --no-pager"
    echo "Deployed $commit: https://$DEPLOY_DOMAIN"
    ;;

  ci-setup)
    # A key that may only run deploy/arena-deploy.sh on the server (no shell), and the GitHub
    # secrets for .github/workflows/deploy.yml, in an environment only the `prod` branch can use.
    # Re-running replaces the key.
    command -v gh >/dev/null || { echo "Needs the GitHub CLI (gh), logged in."; exit 1; }
    host="$(ssh -G "$DEPLOY_HOST" | sed -n 's/^hostname //p')"
    user="$(ssh -G "$DEPLOY_HOST" | sed -n 's/^user //p')"
    port="$(ssh -G "$DEPLOY_HOST" | sed -n 's/^port //p')"
    [ "$port" = 22 ] || { echo "The workflow assumes SSH on port 22, $DEPLOY_HOST uses $port."; exit 1; }
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    ssh-keygen -q -t ed25519 -N "" -C arena-ci -f "$tmp/key"
    echo "Installing the deploy command and the CI key on $DEPLOY_HOST ..."
    upload deploy/arena-deploy.sh /usr/local/bin/arena-deploy
    { echo "restrict,command=\"/usr/local/bin/arena-deploy\" $(cat "$tmp/key.pub")"; } | ssh "$DEPLOY_HOST" "set -e
      chmod 755 /usr/local/bin/arena-deploy
      touch /root/.ssh/authorized_keys
      { grep -v ' arena-ci\$' /root/.ssh/authorized_keys || true; cat; } > /root/.ssh/authorized_keys.new
      chmod 600 /root/.ssh/authorized_keys.new
      mv /root/.ssh/authorized_keys.new /root/.ssh/authorized_keys"
    # The server's host keys, read over the SSH connection you already trust.
    ssh "$DEPLOY_HOST" "cat /etc/ssh/ssh_host_*_key.pub" | awk -v h="$host" '{ print h, $1, $2 }' > "$tmp/known_hosts"
    echo "Setting up the GitHub environment 'production' (prod branch only) and its secrets ..."
    repo="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
    gh api -X PUT "repos/$repo/environments/production" --input - >/dev/null <<'JSON'
{"deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
    policies="repos/$repo/environments/production/deployment-branch-policies"
    gh api "$policies" -q '.branch_policies[].name' | grep -qx prod \
      || gh api -X POST "$policies" -f name=prod >/dev/null
    gh secret set DEPLOY_HOST --env production --body "$user@$host"
    gh secret set DEPLOY_SSH_KEY --env production < "$tmp/key"
    gh secret set DEPLOY_KNOWN_HOSTS --env production < "$tmp/known_hosts"
    echo "Done. Pushing to the prod branch now deploys it (Actions tab: the Deploy workflow)."
    ;;

  logs)
    ssh -t "$DEPLOY_HOST" "journalctl -u arena -f -n 100"
    ;;
esac
