#!/usr/bin/env bash
set -euo pipefail

readonly SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly INSTALL_DIR="/opt/ma/mmonitor"
readonly CONFIG_PATH="$INSTALL_DIR/checks.toml"
readonly DATA_DIR="$INSTALL_DIR/data"
readonly LOG_DIR="$INSTALL_DIR/log"
readonly SERVICE_USER="_mmonitor"
readonly SERVICE_GROUP="_mmonitor"
readonly SERVICE_LABEL="com.ma.mmonitor.collector"
readonly PLIST_PATH="/Library/LaunchDaemons/$SERVICE_LABEL.plist"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "install.sh supports only macOS on Apple Silicon" >&2
  exit 1
fi

for command in brew dscl dseditgroup launchctl sudo uuidgen; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "required command not found: $command" >&2
    exit 1
  fi
done

for file in mmonitor check_mmonitor_memory checks.toml.example "$SERVICE_LABEL.plist"; do
  if [[ ! -e "$SOURCE_DIR/$file" ]]; then
    echo "deployment file not found: $SOURCE_DIR/$file" >&2
    echo "run deploy.sh before install.sh" >&2
    exit 1
  fi
done

if ! brew list --formula monitoring-plugins >/dev/null 2>&1; then
  brew install monitoring-plugins
fi

read_attribute() {
  sudo dscl . -read "$1" "$2" 2>/dev/null \
    | sed -E "s/^(dsAttrTypeNative:)?$2:[[:space:]]*//" \
    | tr '\n' ' ' \
    | xargs
}

create_service_identity() {
  local user_exists=false group_exists=false group_id used_ids id
  sudo dscl . -read "/Users/$SERVICE_USER" >/dev/null 2>&1 && user_exists=true
  sudo dscl . -read "/Groups/$SERVICE_GROUP" >/dev/null 2>&1 && group_exists=true

  if $group_exists; then
    group_id="$(read_attribute "/Groups/$SERVICE_GROUP" PrimaryGroupID)"
    [[ "$group_id" =~ ^[0-9]+$ && "$group_id" -ge 200 && "$group_id" -le 499 \
      && "$(read_attribute "/Groups/$SERVICE_GROUP" RealName)" == "mmonitor service group" ]] || {
      echo "existing $SERVICE_GROUP group has unexpected properties" >&2
      exit 1
    }
  else
    if $user_exists; then
      echo "existing $SERVICE_USER user has no matching group" >&2
      exit 1
    fi
    used_ids="$(sudo dscl . -list /Users UniqueID; sudo dscl . -list /Groups PrimaryGroupID)"
    group_id=""
    for id in $(seq 499 -1 200); do
      if ! awk -v id="$id" '$NF == id { found = 1 } END { exit !found }' <<<"$used_ids"; then
        group_id="$id"
        break
      fi
    done
    [[ -n "$group_id" ]] || {
      echo "no unused local system identity available" >&2
      exit 1
    }
    sudo dscl localhost -create "/Local/Default/Groups/$SERVICE_GROUP"
    sudo dscl localhost -create "/Local/Default/Groups/$SERVICE_GROUP" PrimaryGroupID "$group_id"
    sudo dscl localhost -create "/Local/Default/Groups/$SERVICE_GROUP" RealName "mmonitor service group"
  fi

  if ! $user_exists; then
    if sudo dscl . -list /Users UniqueID | awk -v id="$group_id" '$NF == id { found = 1 } END { exit !found }'; then
      echo "identity $group_id is already used by another user" >&2
      exit 1
    fi
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" UniqueID "$group_id"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" PrimaryGroupID "$group_id"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" GeneratedUID "$(uuidgen)"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" RealName "mmonitor service account"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" NFSHomeDirectory /var/empty
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" UserShell /usr/bin/false
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" IsHidden 1
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" AuthenticationAuthority ";DisabledUser;"
    sudo dscl localhost -create "/Local/Default/Users/$SERVICE_USER" Password "*"
  fi

  [[ "$(read_attribute "/Users/$SERVICE_USER" UniqueID)" == "$group_id" \
    && "$(read_attribute "/Users/$SERVICE_USER" PrimaryGroupID)" == "$group_id" \
    && "$(read_attribute "/Users/$SERVICE_USER" NFSHomeDirectory)" == "/var/empty" \
    && "$(read_attribute "/Users/$SERVICE_USER" UserShell)" == "/usr/bin/false" \
    && "$(read_attribute "/Users/$SERVICE_USER" IsHidden)" == "1" \
    && "$(read_attribute "/Users/$SERVICE_USER" RealName)" == "mmonitor service account" \
    && "$(read_attribute "/Users/$SERVICE_USER" AuthenticationAuthority)" == *"DisabledUser"* \
    && "$(read_attribute "/Users/$SERVICE_USER" Password)" == "*" ]] || {
      echo "existing $SERVICE_USER user has unexpected properties" >&2
      exit 1
    }
  if dseditgroup -o checkmember -m "$SERVICE_USER" admin | grep -q "yes" \
    || sudo -u "$SERVICE_USER" sudo -n true >/dev/null 2>&1; then
    echo "$SERVICE_USER must not have administrator or passwordless sudo rights" >&2
    exit 1
  fi
}

create_service_identity

if sudo launchctl print "system/$SERVICE_LABEL" >/dev/null 2>&1; then
  sudo launchctl bootout "system/$SERVICE_LABEL"
fi

sudo install -d -o root -g wheel -m 0755 "$INSTALL_DIR"
sudo install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0750 "$DATA_DIR" "$LOG_DIR"
while IFS= read -r -d '' mutable_file; do
  metadata="$(sudo stat -f '%HT|%Su|%Sg|%Lp' "$mutable_file")"
  if [[ "$metadata" != "Regular File|$SERVICE_USER|$SERVICE_GROUP|640" ]]; then
    echo "unexpected mutable file: $mutable_file ($metadata)" >&2
    exit 1
  fi
done < <(sudo find -P "$DATA_DIR" "$LOG_DIR" -mindepth 1 -maxdepth 1 -print0)
sudo install -o root -g wheel -m 0755 \
  "$SOURCE_DIR/mmonitor" \
  "$SOURCE_DIR/check_mmonitor_memory" \
  "$INSTALL_DIR/"

if [[ ! -e "$CONFIG_PATH" ]]; then
  sudo install -o root -g "$SERVICE_GROUP" -m 0640 \
    "$SOURCE_DIR/checks.toml.example" "$CONFIG_PATH"
else
  echo "preserving existing configuration: $CONFIG_PATH"
  sudo chown root:"$SERVICE_GROUP" "$CONFIG_PATH"
  sudo chmod 0640 "$CONFIG_PATH"
fi

sudo install -o root -g wheel -m 0644 "$SOURCE_DIR/$SERVICE_LABEL.plist" "$PLIST_PATH"

"$INSTALL_DIR/mmonitor" --help >/dev/null
"$INSTALL_DIR/check_mmonitor_memory" >/dev/null
sudo -u "$SERVICE_USER" "$INSTALL_DIR/mmonitor" \
  --config "$CONFIG_PATH" check system_disk cpu_load memory macos_version >/dev/null

sudo launchctl bootstrap system "$PLIST_PATH"
sudo launchctl kickstart -k "system/$SERVICE_LABEL"

echo "installed mmonitor in $INSTALL_DIR"
echo "configuration: $CONFIG_PATH"
echo "database: $DATA_DIR/mmonitor.sqlite3"
echo "service: $SERVICE_LABEL"
