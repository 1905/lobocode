set -e
terminate() {
  code=$(curl -s -o /dev/null -w '%{http_code}' -m 15 -X DELETE -H "Authorization: Bearer $CONTAINER_API_KEY" "https://console.vast.ai/api/v0/instances/$CONTAINER_ID/") || return 1
  [ "$code" = 200 ] || [ "$code" = 404 ]
}
die() {
  trap - ERR
  echo "lobo bootstrap failed, terminating instance" >&2
  for i in $(seq 1 30); do
    if terminate; then echo "terminate accepted" >&2; break; fi
    echo "terminate failed (try $i), retrying in 20 s" >&2
    sleep 20
  done
  sleep 600
  exit 1
}
trap die ERR
export LOBO_T_BOOT0=$(date +%s.%N)
if [ -z "${LOBO_RELEASE_URL:-}" ] && [ -x /lobo/lobo-agent ]; then
  echo "lobo bootstrap: baked agent, no release download" >&2
  export LOBO_T_APT=$LOBO_T_BOOT0 LOBO_T_ZIP=$LOBO_T_BOOT0
else
  timeout 300 bash -c 'apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unzip >/dev/null'
  export LOBO_T_APT=$(date +%s.%N)
  timeout 300 curl -fsSL "$LOBO_RELEASE_URL" -o /tmp/lobo-release.zip
  echo "$LOBO_RELEASE_SHA256  /tmp/lobo-release.zip" | sha256sum -c
  unzip -o -q /tmp/lobo-release.zip -d /lobo
  export LOBO_T_ZIP=$(date +%s.%N)
fi
# A failed exec (missing or non-executable binary) exits the shell without the ERR trap. execfail
# keeps the shell alive only with errexit off (checked on bash 5.2), then terminate by hand.
shopt -s execfail
set +e
exec /lobo/lobo-agent
die