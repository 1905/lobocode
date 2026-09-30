#!/bin/bash
# All software and model bytes are provided by the image. No boot-time downloads.
set -Eeuo pipefail
terminate() {
  local out payload code
  case "${LOBO_PROVIDER:-runpod}" in
    runpod)
      [ -n "${RUNPOD_POD_ID:-}" ] && [ -n "${RUNPOD_API_KEY:-}" ] || return 1
      payload=$(jq -cn --arg id "$RUNPOD_POD_ID" '{query: ("mutation { podTerminate(input:{podId:" + ($id | tojson) + "}) }")}') || return 1
      out=$(curl -fsS -m 15 https://api.runpod.io/graphql -H "Authorization: Bearer $RUNPOD_API_KEY" -H "Content-Type: application/json" -d "$payload") || return 1
      jq -e '(.errors == null or .errors == []) and (.data | type == "object" and has("podTerminate"))' <<< "$out" > /dev/null ;;
    vast)
      [ -n "${CONTAINER_API_KEY:-}" ] && [[ "${CONTAINER_ID:-}" =~ ^[0-9]+$ ]] || return 1
      code=$(curl -s -o /dev/null -w '%{http_code}' -m 15 -X DELETE -H "Authorization: Bearer $CONTAINER_API_KEY" "https://console.vast.ai/api/v0/instances/$CONTAINER_ID/") || return 1
      [ "$code" = 200 ] || [ "$code" = 404 ] ;;
    *) return 1 ;;
  esac
}
die() {
  trap - ERR TERM INT
  set +e
  echo "lobo image startup failed, terminating instance" >&2
  for ((i=1; i<=30; i++)); do
    if terminate; then
      echo "lobo instance termination accepted" >&2
      exit 1
    fi
    if [ "$i" -lt 30 ]; then sleep 20; fi
  done
  echo "lobo instance termination failed; manual cleanup required" >&2
  exit 1
}
trap die ERR TERM INT
export LOBO_T_BOOT0=$(date +%s.%N)
export LOBO_T_APT=$LOBO_T_BOOT0 LOBO_T_ZIP=$LOBO_T_BOOT0 LOBO_IMAGE_MODEL=1
test -x /lobo/lobo-agent
test -x /app/llama-server
test -r /lobo/model.json
if [ "${LOBO_CONNECTION:-ssh}" = ssh ]; then
  test -x /usr/sbin/sshd
  test -n "${LOBO_CONNECTION_HOST_KEY:-}"
  test -n "${LOBO_CONNECTION_PUBLIC_KEY:-}"
  umask 077
  mkdir -p /lobo/cloud-ssh /run/sshd
  printf '%s' "$LOBO_CONNECTION_HOST_KEY" | base64 -d > /lobo/cloud-ssh/host
  printf 'restrict,port-forwarding,permitopen="127.0.0.1:8080",permitopen="127.0.0.1:8081" %s\n' "$LOBO_CONNECTION_PUBLIC_KEY" > /lobo/cloud-ssh/authorized_keys
  cat > /lobo/cloud-ssh/config <<'LOBO_SSH_CONFIG'
Port 2222
ListenAddress 0.0.0.0
HostKey /lobo/cloud-ssh/host
PidFile /lobo/cloud-ssh/sshd.pid
AuthorizedKeysFile /lobo/cloud-ssh/authorized_keys
PermitRootLogin prohibit-password
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM yes
AllowUsers root
AllowTcpForwarding local
PermitOpen 127.0.0.1:8080 127.0.0.1:8081
AllowAgentForwarding no
X11Forwarding no
PermitTTY no
ForceCommand /bin/false
LOBO_SSH_CONFIG
  /usr/sbin/sshd -f /lobo/cloud-ssh/config
  unset LOBO_CONNECTION_HOST_KEY LOBO_CONNECTION_PUBLIC_KEY
else
  test -x /lobo/bin/cloudflared
fi
shopt -s execfail
set +e
exec /lobo/lobo-agent
die
