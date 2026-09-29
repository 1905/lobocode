use crate::{Error, Result};
use lobo_proto::{Gpu, Host, Llama};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

pub const SMI_ARGS: [&str; 2] = [
    "--query-gpu=name,memory.used,memory.total,utilization.gpu",
    "--format=csv,noheader,nounits",
];
const READ_TIMEOUT: Duration = Duration::from_secs(3);

pub fn parse_llama(text: &str) -> Result<Llama> {
    let mut values = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err(Error::msg(format!("metrics: bad line {line:?}")));
        }
        let value = fields[1]
            .parse::<f64>()
            .map_err(|_| Error::msg(format!("metrics: bad value in {line:?}")))?;
        values.insert(
            fields[0].strip_prefix("llamacpp:").unwrap_or(fields[0]),
            value,
        );
    }
    for key in ["requests_processing", "prompt_tokens_total"] {
        if !values.contains_key(key) {
            return Err(Error::msg(format!("metrics: missing llamacpp:{key}")));
        }
    }
    let get = |key| values.get(key).copied().unwrap_or_default();
    Ok(Llama {
        requests_processing: get("requests_processing") as i64,
        requests_deferred: get("requests_deferred") as i64,
        prompt_tokens_total: get("prompt_tokens_total") as i64,
        gen_tokens_total: get("tokens_predicted_total") as i64,
        prompt_tps: get("prompt_tokens_seconds"),
        gen_tps: get("predicted_tokens_seconds"),
    })
}

pub fn parse_nvidia_smi(csv: &str) -> Result<Gpu> {
    let line = csv.trim().lines().next().unwrap_or_default().trim();
    let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
    if fields.len() != 4 {
        return Err(Error::msg(format!(
            "nvidia-smi: want 4 fields, got {line:?}"
        )));
    }
    let mut nums = [0; 3];
    for (n, field) in nums.iter_mut().zip(&fields[1..]) {
        *n = field
            .parse()
            .map_err(|_| Error::msg(format!("nvidia-smi: bad number in {line:?}")))?;
    }
    Ok(Gpu {
        name: fields[0].to_owned(),
        vram_used_mb: nums[0],
        vram_total_mb: nums[1],
        util_pct: nums[2],
    })
}

pub fn parse_host(loadavg: &str, meminfo: &str) -> Result<Host> {
    let fields = loadavg.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 3 {
        return Err(Error::msg(format!("loadavg: bad {loadavg:?}")));
    }
    let mut loads = [0.0; 3];
    for (load, field) in loads.iter_mut().zip(&fields[..3]) {
        *load = field
            .parse()
            .map_err(|_| Error::msg(format!("loadavg: bad {loadavg:?}")))?;
    }
    let mut memory = BTreeMap::new();
    for line in meminfo.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(key), Some(value)) = (fields.next(), fields.next())
            && let Ok(value) = value.parse::<i64>()
        {
            memory.insert(key.trim_end_matches(':'), value);
        }
    }
    let (Some(total), Some(available)) = (memory.get("MemTotal"), memory.get("MemAvailable"))
    else {
        return Err(Error::msg("meminfo: missing MemTotal/MemAvailable"));
    };
    Ok(Host {
        load1: loads[0],
        load5: loads[1],
        load15: loads[2],
        mem_total_mb: total / 1024,
        mem_used_mb: (total - available) / 1024,
    })
}

pub struct Collector {
    pub llama_url: String,
    pub api_key: String,
    pub http: reqwest::Client,
    pub nvidia_smi: PathBuf,
    pub proc_dir: PathBuf,
}

impl Collector {
    pub async fn llama(&self) -> Result<Llama> {
        let response = self
            .http
            .get(format!("{}/metrics", self.llama_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .timeout(READ_TIMEOUT)
            .send()
            .await
            .map_err(|e| Error::msg(e.without_url().to_string()))?;
        if response.status() != 200 {
            return Err(Error::msg(format!(
                "metrics: HTTP {}",
                response.status().as_u16()
            )));
        }
        let text = response
            .text()
            .await
            .map_err(|e| Error::msg(e.without_url().to_string()))?;
        parse_llama(&text)
    }
    pub async fn gpu(&self) -> Result<Gpu> {
        let binary = if self.nvidia_smi.as_os_str().is_empty() {
            std::path::Path::new("nvidia-smi")
        } else {
            &self.nvidia_smi
        };
        let output = tokio::time::timeout(
            READ_TIMEOUT,
            tokio::process::Command::new(binary)
                .args(SMI_ARGS)
                .env_clear()
                .envs(crate::process::clean_env())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| Error::msg("nvidia-smi: timeout"))??;
        if !output.status.success() {
            return Err(Error::msg(format!("nvidia-smi: {}", output.status)));
        }
        parse_nvidia_smi(&String::from_utf8_lossy(&output.stdout))
    }
    pub async fn host(&self) -> Result<Host> {
        let dir = if self.proc_dir.as_os_str().is_empty() {
            std::path::Path::new("/proc")
        } else {
            &self.proc_dir
        };
        tokio::time::timeout(READ_TIMEOUT, async {
            let load = tokio::fs::read_to_string(dir.join("loadavg")).await?;
            let memory = tokio::fs::read_to_string(dir.join("meminfo")).await?;
            parse_host(&load, &memory)
        })
        .await
        .map_err(|_| Error::msg("host metrics: timeout"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, path},
    };

    const SAMPLE: &str = "# HELP llamacpp:prompt_tokens_total Number of prompt tokens processed.\n# TYPE llamacpp:prompt_tokens_total counter\nllamacpp:prompt_tokens_total 182340\nllamacpp:tokens_predicted_total 21044\nllamacpp:prompt_tokens_seconds 2410.5\nllamacpp:predicted_tokens_seconds 48.3\nllamacpp:requests_processing 1\nllamacpp:requests_deferred 2\n";
    const MEMORY: &str =
        "MemTotal: 461373440 kB\nMemFree: 271000000 kB\nMemAvailable: 391000000 kB\n";
    #[test]
    fn parse_llama() {
        assert_eq!(
            super::parse_llama(SAMPLE).unwrap(),
            Llama {
                requests_processing: 1,
                requests_deferred: 2,
                prompt_tokens_total: 182340,
                gen_tokens_total: 21044,
                prompt_tps: 2410.5,
                gen_tps: 48.3
            }
        );
        for s in [
            "llamacpp:tokens_predicted_total 5\n",
            "llamacpp:requests_processing abc\n",
            "llamacpp:requests_processing\n",
        ] {
            assert!(super::parse_llama(s).is_err());
        }
    }
    #[test]
    fn parse_llama_fixture() {
        super::parse_llama(include_str!("../testdata/llama_metrics.txt")).unwrap();
    }
    #[test]
    fn parse_nvidia_smi() {
        assert_eq!(
            super::parse_nvidia_smi("NVIDIA GeForce RTX 5090, 30112, 32607, 3\n").unwrap(),
            Gpu {
                name: "NVIDIA GeForce RTX 5090".into(),
                vram_used_mb: 30112,
                vram_total_mb: 32607,
                util_pct: 3
            }
        );
        assert!(super::parse_nvidia_smi("NVIDIA, x, 1, 2").is_err());
    }
    #[test]
    fn parse_host() {
        assert_eq!(
            super::parse_host("1.20 0.90 0.70 2/1234 5678\n", MEMORY).unwrap(),
            Host {
                load1: 1.2,
                load5: 0.9,
                load15: 0.7,
                mem_total_mb: 461373440 / 1024,
                mem_used_mb: (461373440 - 391000000) / 1024
            }
        );
        assert!(super::parse_host("x", MEMORY).is_err());
        assert!(super::parse_host("1 1 1", "MemTotal: 5 kB\n").is_err());
    }
    #[tokio::test]
    async fn collector() {
        let server = MockServer::start().await;
        Mock::given(path("/metrics"))
            .and(header("authorization", "Bearer k"))
            .and(header("user-agent", crate::http::USER_AGENT.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("loadavg"), "0.5 0.4 0.3 1/2 3").unwrap();
        std::fs::write(temp.path().join("meminfo"), MEMORY).unwrap();
        let smi = temp.path().join("nvidia-smi");
        std::fs::write(&smi, "#!/bin/sh\necho 'NVIDIA GeForce RTX 5090, 1, 2, 3'\n").unwrap();
        std::fs::set_permissions(&smi, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut c = Collector {
            llama_url: server.uri(),
            api_key: "k".into(),
            http: crate::http::client(),
            nvidia_smi: smi,
            proc_dir: temp.path().into(),
        };
        assert_eq!(c.llama().await.unwrap().prompt_tokens_total, 182340);
        assert_eq!(c.gpu().await.unwrap().util_pct, 3);
        assert_eq!(c.host().await.unwrap().load1, 0.5);
        c.api_key = "bad".into();
        assert!(c.llama().await.is_err());
    }
    #[tokio::test]
    async fn collector_timeout() {
        let server = MockServer::start().await;
        Mock::given(path("/metrics"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
            .mount(&server)
            .await;
        let c = Collector {
            llama_url: server.uri(),
            api_key: "k".into(),
            http: crate::http::client(),
            nvidia_smi: PathBuf::new(),
            proc_dir: PathBuf::new(),
        };
        let start = std::time::Instant::now();
        assert!(c.llama().await.is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
