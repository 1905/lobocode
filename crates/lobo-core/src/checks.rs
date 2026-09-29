use crate::{Error, Result};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};
use tokio_util::io::StreamReader;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Function {
    arguments: Value,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolCall {
    function: Function,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolMessage {
    tool_calls: Option<Vec<ToolCall>>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolChoice {
    finish_reason: String,
    message: ToolMessage,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ToolReply {
    choices: Option<Vec<ToolChoice>>,
}

pub fn validate_tool_call(body: &[u8]) -> Result<()> {
    let reply: ToolReply = serde_json::from_slice(body)
        .map_err(|e| Error::Other(format!("tool call: bad json: {e}")))?;
    let choice = reply
        .choices
        .and_then(|v| v.into_iter().next())
        .ok_or_else(|| Error::Other("tool call: no choices".into()))?;
    let call = choice
        .message
        .tool_calls
        .and_then(|v| v.into_iter().next())
        .ok_or_else(|| {
            Error::Other(format!(
                "tool call: no tool_calls (finish_reason {:?})",
                choice.finish_reason
            ))
        })?;
    if choice.finish_reason != "tool_calls" {
        return Err(Error::Other(format!(
            "tool call: finish_reason {:?}, want tool_calls",
            choice.finish_reason
        )));
    }
    let s = call.function.arguments.as_str().ok_or_else(|| {
        Error::Other(format!(
            "tool call: arguments is not a JSON string: {}",
            call.function.arguments
        ))
    })?;
    // Go accepts a JSON null as a nil map; preserve that edge case.
    if !matches!(
        serde_json::from_str::<Value>(s),
        Ok(Value::Object(_) | Value::Null)
    ) {
        return Err(Error::Other(format!(
            "tool call: arguments string is not a JSON object: {s:?}"
        )));
    }
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Delta {
    content: String,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Choice {
    finish_reason: Option<String>,
    delta: Delta,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Chunk {
    error: Value,
    choices: Option<Vec<Choice>>,
}

pub async fn read_stream<R: AsyncBufRead + Unpin>(mut r: R) -> Result<String> {
    const MAX_LINE: u64 = 1 << 20;
    let mut text = String::new();
    let mut finish = String::new();
    let mut done = false;
    let mut line = Vec::new();
    loop {
        line.clear();
        if (&mut r).take(MAX_LINE).read_until(b'\n', &mut line).await? == 0 {
            break;
        }
        if line.len() as u64 == MAX_LINE && line.last() != Some(&b'\n') {
            return Err(Error::Other("bufio.Scanner: token too long".into()));
        }
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches(['\n', '\r']);
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            done = true;
            break;
        }
        let chunk: Chunk = serde_json::from_str(data)
            .map_err(|_| Error::Other(format!("chat: malformed chunk {data:?}")))?;
        if chunk.error != Value::Null {
            return Err(Error::Other(format!("chat: error event {}", chunk.error)));
        }
        if let Some(choice) = chunk.choices.and_then(|v| v.into_iter().next()) {
            text.push_str(&choice.delta.content);
            if let Some(reason) = choice.finish_reason {
                finish = reason;
            }
        }
    }
    if !done {
        return Err(Error::Other("chat: stream ended without [DONE]".into()));
    }
    if finish != "stop" && finish != "length" {
        return Err(Error::Other(format!("chat: finish_reason {finish:?}")));
    }
    if text.is_empty() {
        return Err(Error::Other("chat: empty streamed reply".into()));
    }
    Ok(text)
}

async fn post(
    hc: &reqwest::Client,
    base: &str,
    key: &str,
    body: Value,
) -> Result<reqwest::Response> {
    let res = hc
        .post(format!("{}/chat/completions", base.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&body)
        .send()
        .await?;
    if res.status() != 200 {
        let status = res.status().as_u16();
        let text = res.text().await?;
        return Err(Error::Other(format!("HTTP {status}: {}", text.trim())));
    }
    Ok(res)
}
pub async fn chat(hc: &reqwest::Client, base_url: &str, key: &str, model: &str) -> Result<String> {
    let response = post(
        hc,
        base_url,
        key,
        json!({"model":model,"stream":true,"temperature":0.2,"max_tokens":200,
        "messages":[{"role":"user","content":"Explain Go channels in 3 concise bullet points."}]}),
    )
    .await?;
    let reader = StreamReader::new(response.bytes_stream().map_err(std::io::Error::other));
    read_stream(reader).await
}
pub async fn tool_call(
    hc: &reqwest::Client,
    base_url: &str,
    key: &str,
    model: &str,
) -> Result<Vec<u8>> {
    let response = post(hc,base_url,key,json!({"model":model,"stream":false,"tool_choice":"auto",
        "messages":[{"role":"user","content":"Use the get_weather tool to check Bali."}],
        "tools":[{"type":"function","function":{"name":"get_weather","description":"Get weather for a location",
            "parameters":{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}}}]})).await?;
    Ok(response.bytes().await?.to_vec())
}

#[cfg(test)]
mod tests;
