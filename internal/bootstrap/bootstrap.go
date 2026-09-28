// Package bootstrap builds the container start script and env shared by every provider.
package bootstrap

import (
	"strconv"
	"time"

	"github.com/1905/lobocode/internal/provider"
)

// terminate() per provider: delete this instance with the credentials the provider injects into the
// container, so a broken boot can't bill. Returns non-zero unless the provider accepted it.
// RunPod: pod-scoped key over GraphQL (REST refuses it). Vast: instance-scoped CONTAINER_API_KEY over
// REST (verified live 2026-09-25); 404 = already gone.
var terminateCmd = map[string]string{
	"runpod": `out=$(curl -s -m 15 https://api.runpod.io/graphql -H "Authorization: Bearer $RUNPOD_API_KEY" -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation { podTerminate(input:{podId:\\\"$RUNPOD_POD_ID\\\"}) }\"}") || return 1
  case "$out" in *'"errors"'*) return 1 ;; *'"data"'*) return 0 ;; esac
  return 1`,
	"vast": `code=$(curl -s -o /dev/null -w '%{http_code}' -m 15 -X DELETE -H "Authorization: Bearer $CONTAINER_API_KEY" "https://console.vast.ai/api/v0/instances/$CONTAINER_ID/") || return 1
  [ "$code" = 200 ] || [ "$code" = 404 ]`,
}

// Script is the bootstrap: install unzip, fetch + verify the release zip, exec the agent.
// Every step is time-bounded; any failure terminates the instance, retried until the provider accepts.
func Script(providerName string) string {
	return `set -e
terminate() {
  ` + terminateCmd[providerName] + `
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
timeout 300 bash -c 'apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unzip >/dev/null'
export LOBO_T_APT=$(date +%s.%N)
timeout 300 curl -fsSL "$LOBO_RELEASE_URL" -o /tmp/lobo-release.zip
echo "$LOBO_RELEASE_SHA256  /tmp/lobo-release.zip" | sha256sum -c
unzip -o -q /tmp/lobo-release.zip -d /lobo
export LOBO_T_ZIP=$(date +%s.%N)
# A failed exec (missing or non-executable binary) exits the shell without the ERR trap. execfail
# keeps the shell alive only with errexit off (checked on bash 5.2), then terminate by hand.
shopt -s execfail
set +e
exec /lobo/lobo-agent
die`
}

// Env is the container env: never R2 or account keys.
func Env(o provider.CreateOpts, providerName string) map[string]string {
	env := map[string]string{
		"LOBO_PROVIDER":       providerName,
		"LOBO_RELEASE_URL":    o.ReleaseURL,
		"LOBO_RELEASE_SHA256": o.ReleaseSHA256,
		"LOBO_MODEL_URL":      o.ModelURL,
		"LOBO_API_KEY":        o.LoboAPIKey,
		"CF_TUNNEL_TOKEN":     o.CFTunnelToken,
		"LOBO_MODEL":          o.Model,
		"LOBO_CTX":            strconv.Itoa(o.Ctx),
		"LOBO_IDLE_MIN":       strconv.Itoa(o.IdleMin),
		"LOBO_EXPIRES_AT":     o.ExpiresAt.UTC().Format(time.RFC3339),
		"LOBO_BOOT_TIMEOUT":   "40m",
	}
	if o.BootID != "" {
		env["LOBO_BOOT_ID"] = o.BootID
	}
	if o.ModelFallback != "" {
		env["LOBO_MODEL_URL_FALLBACK"] = o.ModelFallback
	}
	if o.MinMBps > 0 {
		env["LOBO_MIN_MBPS"] = strconv.Itoa(o.MinMBps)
	}
	if o.DLConns > 0 {
		env["LOBO_DL_CONNS"] = strconv.Itoa(o.DLConns)
	}
	if o.ModelSSHKey != "" {
		env["LOBO_MODEL_SSH_KEY"] = o.ModelSSHKey
		env["LOBO_MODEL_SSH_HOSTKEY"] = o.ModelHostKey
	}
	return env
}
