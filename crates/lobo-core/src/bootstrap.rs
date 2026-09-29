use crate::provider::CreateOpts;
use chrono::SecondsFormat;
use std::collections::BTreeMap;

const RUNPOD_TERMINATE: &str = r###"out=$(curl -s -m 15 https://api.runpod.io/graphql -H "Authorization: Bearer $RUNPOD_API_KEY" -H "Content-Type: application/json" \
    -d "{\"query\":\"mutation { podTerminate(input:{podId:\\\"$RUNPOD_POD_ID\\\"}) }\"}") || return 1
  case "$out" in *'"errors"'*) return 1 ;; *'"data"'*) return 0 ;; esac
  return 1"###;

const VAST_TERMINATE: &str = r###"code=$(curl -s -o /dev/null -w '%{http_code}' -m 15 -X DELETE -H "Authorization: Bearer $CONTAINER_API_KEY" "https://console.vast.ai/api/v0/instances/$CONTAINER_ID/") || return 1
  [ "$code" = 200 ] || [ "$code" = 404 ]"###;

const SCRIPT_START: &str = r###"set -e
terminate() {
  "###;

const SCRIPT_END: &str = r###"
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
die"###;

pub fn script(provider: &str) -> String {
    let terminate = match provider {
        "runpod" => RUNPOD_TERMINATE,
        "vast" => VAST_TERMINATE,
        _ => "",
    };
    format!("{SCRIPT_START}{terminate}{SCRIPT_END}")
}

pub fn env(o: &CreateOpts, provider: &str) -> BTreeMap<String, String> {
    let mut values: BTreeMap<String, String> = [
        ("LOBO_PROVIDER", provider.to_owned()),
        ("LOBO_MODEL_URL", o.model_url.clone()),
        ("LOBO_API_KEY", o.lobo_api_key.clone()),
        ("CF_TUNNEL_TOKEN", o.cf_tunnel_token.clone()),
        ("LOBO_MODEL", o.model.clone()),
        ("LOBO_CTX", o.ctx.to_string()),
        ("LOBO_IDLE_MIN", o.idle_min.to_string()),
        (
            "LOBO_EXPIRES_AT",
            o.expires_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        ),
        ("LOBO_BOOT_TIMEOUT", "40m".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect();
    if !o.release_url.is_empty() {
        values.insert("LOBO_RELEASE_URL".into(), o.release_url.clone());
        values.insert("LOBO_RELEASE_SHA256".into(), o.release_sha256.clone());
    }
    for (key, value) in [
        ("LOBO_BOOT_ID", &o.boot_id),
        ("LOBO_MODEL_URL_FALLBACK", &o.model_fallback),
    ] {
        if !value.is_empty() {
            values.insert(key.into(), value.clone());
        }
    }
    for (key, value) in [("LOBO_MIN_MBPS", o.min_mbps), ("LOBO_DL_CONNS", o.dl_conns)] {
        if value > 0 {
            values.insert(key.into(), value.to_string());
        }
    }
    if !o.model_ssh_key.is_empty() {
        values.insert("LOBO_MODEL_SSH_KEY".into(), o.model_ssh_key.clone());
        values.insert("LOBO_MODEL_SSH_HOSTKEY".into(), o.model_host_key.clone());
    }
    values
}

#[cfg(test)]
mod tests;
