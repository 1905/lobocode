use super::{
    prompt::{PromptError, PromptResult, Prompter},
    state::{WizardState, new_api_key},
    validate::*,
};
use std::{collections::BTreeMap, path::Path};

pub fn run_wizard(
    p: &mut dyn Prompter,
    path: &Path,
    cur: BTreeMap<String, String>,
) -> anyhow::Result<Option<BTreeMap<String, String>>> {
    match flow(p, path, WizardState::new(cur)) {
        Ok(s) if s.save => Ok(Some(s.result(&new_api_key()))),
        Ok(_) | Err(PromptError::Aborted) => Ok(None),
        Err(PromptError::Failed(e)) => Err(e),
    }
}
fn secret_desc(help: &str, current: &str) -> String {
    format!(
        "{help}\nNow: {}. Leave empty to keep it, type - to remove it.",
        lobo_core::config::mask(current)
    )
}
fn flow(p: &mut dyn Prompter, path: &Path, mut s: WizardState) -> PromptResult<WizardState> {
    p.note("lobo config",&format!("File: {}\nPlain KEY=value lines. You can edit it by hand at any time;\nthis form only rewrites the keys it shows and keeps everything else.\n\nEnter: next · Esc: quit without saving",path.display()))?;
    s.runpod = p.secret(
        "1/4 · GPU providers · RunPod API key",
        &secret_desc(
            "runpod.io → Settings → API keys.",
            s.current("RUNPOD_API_KEY"),
        ),
        &|_| Ok(()),
    )?;
    s.vast = p.secret(
        "1/4 · GPU providers · Vast.ai API key",
        &secret_desc(
            "cloud.vast.ai → Account → API keys. Optional.",
            s.current("VASTAI_API_KEY"),
        ),
        &|v| s.provider_keys(v),
    )?;
    if lobo_core::config::Laptop::from_values(&s.cur)
        .connection_mode()
        .ok()
        == Some("cloudflare")
    {
        let domain = p.text(
            "2/4 · Access · Domain",
            "The hostname your Cloudflare tunnel serves, e.g. lobo.example.com. Cloud only.",
            &s.domain,
            &hostname,
        )?;
        s.domain = domain;
    }
    let opts = if s.current("LOBO_API_KEY").is_empty() {
        vec![("generate a key", "new")]
    } else {
        vec![
            ("keep the current key", "keep"),
            ("generate a new key", "new"),
        ]
    };
    s.api_key = p.select(
        "2/4 · Access · LOBO API key",
        &format!(
            "Clients (OpenCode etc.) send this as the Bearer key. Now: {}.",
            lobo_core::config::mask(s.current("LOBO_API_KEY"))
        ),
        &opts,
        &s.api_key,
    )?;
    if lobo_core::config::Laptop::from_values(&s.cur)
        .connection_mode()
        .ok()
        == Some("cloudflare")
    {
        s.tunnel = p.secret(
            "2/4 · Access · Cloudflare tunnel token",
            &secret_desc(
                "Zero Trust → Networks → Tunnels → your tunnel → token. Cloud only.",
                s.current("CF_TUNNEL_TOKEN"),
            ),
            &|v| s.tunnel_token(v),
        )?;
    }
    if s.both_keys() {
        s.provider = p.select(
            "3/4 · Provider · Default provider",
            "Both keys are set. `lobo up --provider …` still overrides this.",
            &provider_options(),
            &s.provider,
        )?;
    }
    s.ctx = p.text(
        "3/4 · Defaults for lobo up · Context size",
        "Tokens. Empty = release default (65536).",
        &s.ctx,
        &*whole_number(512),
    )?;
    s.idle = p.text(
        "3/4 · Defaults for lobo up · Idle minutes",
        "The pod deletes itself after this long without requests. Empty = 30.",
        &s.idle,
        &*whole_number(1),
    )?;
    s.max_h = p.text(
        "3/4 · Defaults for lobo up · Max hours",
        "Hard lifetime of a pod. Empty = 12.",
        &s.max_h,
        &*whole_number(1),
    )?;
    if !s.runpod_key().is_empty() {
        s.cloud = p.select(
            "3/4 · RunPod · RunPod cloud",
            "Community = community hosts only. Secure = datacenter first, community fallback.",
            &[
                ("Community (cheapest, $0.69/h)", "community"),
                ("Secure (datacenter first, $0.99/h)", "secure"),
            ],
            &s.cloud,
        )?;
    }
    if !s.vast_key().is_empty() {
        s.vast_dph = p.text(
            "3/4 · Vast.ai · Vast max price, $/h",
            "Offers above this are skipped. Empty = 1.20.",
            &s.vast_dph,
            &positive_float,
        )?;
    }
    s.save = p.confirm(
        &format!("4/4 · Save · Save to {}?", path.display()),
        &s.summary(false),
        "Save",
        "Discard",
        s.save,
    )?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Scripted {
        titles: Vec<String>,
        both: bool,
        abort_at: Option<usize>,
        discard: bool,
        reject_ctx: bool,
        checks: usize,
    }
    impl Scripted {
        fn record(&mut self, title: &str) -> PromptResult<()> {
            self.titles.push(title.into());
            if self.abort_at == Some(self.titles.len()) {
                Err(PromptError::Aborted)
            } else {
                Ok(())
            }
        }
    }
    impl Prompter for Scripted {
        fn note(&mut self, t: &str, _: &str) -> PromptResult<()> {
            self.record(t)
        }
        fn secret(
            &mut self,
            t: &str,
            _: &str,
            check: &dyn Fn(&str) -> Result<(), String>,
        ) -> PromptResult<String> {
            self.record(t)?;
            let value = if t.contains("RunPod API") {
                "rp"
            } else if t.contains("Cloudflare") {
                "token"
            } else if self.both {
                "vast"
            } else {
                ""
            };
            check(value).unwrap();
            Ok(value.into())
        }
        fn text(
            &mut self,
            t: &str,
            _: &str,
            initial: &str,
            check: &dyn Fn(&str) -> Result<(), String>,
        ) -> PromptResult<String> {
            self.record(t)?;
            if self.reject_ctx && t.ends_with("Context size") {
                assert!(check("100").is_err());
                self.checks += 1;
                assert!(check("8192").is_ok());
                self.checks += 1;
                return Ok("8192".into());
            }
            let value = if t.ends_with("Domain") {
                "lobo.x.cc"
            } else if t.ends_with("Bucket URL") {
                "https://pub.r2.dev"
            } else {
                initial
            };
            check(value).unwrap();
            Ok(value.into())
        }
        fn select(
            &mut self,
            t: &str,
            _: &str,
            opts: &[(&str, &str)],
            cur: &str,
        ) -> PromptResult<String> {
            self.record(t)?;
            assert!(opts.iter().any(|(_, v)| *v == cur));
            Ok(cur.into())
        }
        fn confirm(
            &mut self,
            t: &str,
            desc: &str,
            _: &str,
            _: &str,
            _: bool,
        ) -> PromptResult<bool> {
            self.record(t)?;
            assert!(!desc.lines().any(|s| s.ends_with(" token")));
            Ok(!self.discard)
        }
    }
    #[test]
    fn flow_contains_only_cloud_controls() {
        let mut p = Scripted::default();
        let result = run_wizard(&mut p, Path::new("config"), BTreeMap::new())
            .unwrap()
            .unwrap();
        assert!(result["LOBO_API_KEY"].starts_with("sk-"));
        let fields = p
            .titles
            .iter()
            .map(|s| s.rsplit(" · ").next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            [
                "lobo config",
                "RunPod API key",
                "Vast.ai API key",
                "LOBO API key",
                "Context size",
                "Idle minutes",
                "Max hours",
                "RunPod cloud",
                "Save to config?"
            ]
        );
    }
    #[test]
    fn flow_pick_with_two_keys_and_validation() {
        let mut p = Scripted {
            both: true,
            reject_ctx: true,
            ..Default::default()
        };
        let r = run_wizard(&mut p, Path::new("config"), BTreeMap::new())
            .unwrap()
            .unwrap();
        assert_eq!(r["LOBO_CTX"], "8192");
        assert_eq!(p.checks, 2);
        assert_eq!(p.titles[4], "3/4 · Provider · Default provider");
        assert!(p.titles.iter().any(|s| s.ends_with("Vast max price, $/h")));
    }
    #[test]
    fn abort_and_discard_save_nothing() {
        for abort in [Some(1), Some(5), Some(9), None] {
            let mut p = Scripted {
                abort_at: abort,
                discard: true,
                ..Default::default()
            };
            assert!(
                run_wizard(&mut p, Path::new("config"), BTreeMap::new())
                    .unwrap()
                    .is_none()
            );
        }
    }
}
