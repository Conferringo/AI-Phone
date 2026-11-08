//! Native Messages transport mapping. Opaque thinking blocks remain private replay data.
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

fn content(value: &Value) -> Result<Vec<Value>, String> {
    if let Some(text) = value.as_str() {
        return Ok(vec![json!({"type":"text","text":text})]);
    }
    let mut out = vec![];
    for part in value.as_array().ok_or("消息内容格式无效")? {
        match part["type"].as_str() {
            Some("input_text" | "output_text" | "text") => {
                if let Some(text) = part["text"].as_str().filter(|s| !s.is_empty()) {
                    out.push(json!({"type":"text","text":text}));
                }
            }
            Some("input_image") => {
                let url = part["image_url"].as_str().ok_or("图片地址无效")?;
                if let Some(raw) = url.strip_prefix("data:") {
                    let (media, data) = raw.split_once(";base64,").ok_or("图片编码无效")?;
                    if !["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&media) {
                        return Err("图片格式不受支持".into());
                    }
                    out.push(json!({"type":"image","source":{"type":"base64","media_type":media,"data":data}}));
                } else if reqwest::Url::parse(url).is_ok_and(|u| u.scheme() == "https") {
                    out.push(json!({"type":"image","source":{"type":"url","url":url}}));
                } else {
                    return Err("图片地址须为HTTPS或内嵌图片".into());
                }
            }
            _ => return Err("Messages协议不支持该内容类型".into()),
        }
    }
    Ok(out)
}
fn append(messages: &mut Vec<Value>, role: &str, mut blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut().filter(|v| v["role"] == role) {
        last["content"].as_array_mut().unwrap().append(&mut blocks);
    } else {
        messages.push(json!({"role":role,"content":blocks}));
    }
}
pub fn request_body(body: &Value) -> Result<Value, String> {
    let mut messages = vec![];
    for item in body["input"].as_array().ok_or("模型请求缺少上下文")? {
        match item["type"].as_str() {
            Some("reasoning") => {
                if let Some(blocks) = item["anthropic_blocks"].as_array() {
                    append(&mut messages, "assistant", blocks.clone());
                }
            }
            Some("function_call") => {
                let input: Value =
                    serde_json::from_str(item["arguments"].as_str().ok_or("工具参数无效")?)
                        .map_err(|_| "工具参数无效")?;
                if !input.is_object() {
                    return Err("工具参数必须是对象".into());
                }
                append(
                    &mut messages,
                    "assistant",
                    vec![
                        json!({"type":"tool_use","id":item["call_id"],"name":item["name"],"input":input}),
                    ],
                );
            }
            Some("function_call_output") => {
                let output = if item["output"].is_array() || item["output"].is_string() {
                    content(&item["output"])?
                } else {
                    vec![json!({"type":"text","text":item["output"].to_string()})]
                };
                append(
                    &mut messages,
                    "user",
                    vec![
                        json!({"type":"tool_result","tool_use_id":item["call_id"],"content":output}),
                    ],
                );
            }
            _ => {
                let role = item["role"].as_str().ok_or("上下文角色无效")?;
                let mut blocks = content(&item["content"])?;
                // Preserve the chronology of mid-run context checkpoints; runtime policy is in `system`.
                if ["system", "developer"].contains(&role) {
                    blocks.insert(
                        0,
                        json!({"type":"text","text":"Context checkpoint from the local runtime:"}),
                    );
                } else if !["user", "assistant"].contains(&role) {
                    return Err("上下文角色无效".into());
                }
                append(
                    &mut messages,
                    if role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    },
                    blocks,
                );
            }
        }
    }
    let mut result = json!({"model":body["model"],"stream":true,"max_tokens":body.get("max_output_tokens").and_then(Value::as_u64).unwrap_or(8192),"messages":messages});
    if let Some(system) = body["instructions"].as_str() {
        result["system"] = json!(system);
    }
    if let Some(tools) = body["tools"].as_array().filter(|v| !v.is_empty()) {
        let mut mapped = vec![];
        for tool in tools {
            if tool["type"] != "function" {
                return Err("Messages协议仅支持函数工具".into());
            }
            mapped.push(json!({"name":tool["name"],"description":tool["description"],"input_schema":tool["parameters"]}));
        }
        result["tools"] = json!(mapped);
        result["tool_choice"] = match body.get("tool_choice") {
            Some(choice) if choice.is_object() => {
                json!({"type":"tool","name":choice["name"],"disable_parallel_tool_use":true})
            }
            Some(choice) if choice == "required" => {
                json!({"type":"any","disable_parallel_tool_use":true})
            }
            Some(choice) if choice == "none" => json!({"type":"none"}),
            _ => json!({"type":"auto","disable_parallel_tool_use":true}),
        };
    }
    Ok(result)
}
#[derive(Default)]
struct Block {
    value: Value,
    json: String,
    closed: bool,
}
#[derive(Default)]
pub struct MessagesStream {
    sse: crate::model_sse::SseDecoder,
    started: bool,
    ended: bool,
    blocks: BTreeMap<u64, Block>,
    stop: Option<String>,
}
impl MessagesStream {
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Option<Vec<Value>>, String> {
        for data in self.sse.feed(chunk)? {
            if data.is_empty() {
                continue;
            }
            if self.ended {
                return Err("模型结束后仍返回内容".into());
            }
            let event: Value = serde_json::from_str(&data).map_err(|_| "模型流格式无效")?;
            match event["type"].as_str() {
                Some("ping") => {}
                Some("error") => return Err("Messages模型流返回错误，未执行后续操作".into()),
                Some("message_start") => {
                    if self.started || event["message"]["role"] != "assistant" {
                        return Err("模型流起始状态无效".into());
                    }
                    self.started = true;
                }
                Some("content_block_start") => {
                    if !self.started || self.stop.is_some() {
                        return Err("模型内容顺序无效".into());
                    }
                    let id = event["index"]
                        .as_u64()
                        .filter(|n| *n < 256)
                        .ok_or("模型内容编号无效")?;
                    if self.blocks.contains_key(&id) {
                        return Err("模型内容编号重复".into());
                    }
                    let block = event["content_block"].clone();
                    if !matches!(
                        block["type"].as_str(),
                        Some("text" | "tool_use" | "thinking" | "redacted_thinking")
                    ) {
                        return Err("模型返回不支持的内容类型".into());
                    }
                    self.blocks.insert(
                        id,
                        Block {
                            value: block,
                            ..Default::default()
                        },
                    );
                }
                Some("content_block_delta") => {
                    let id = event["index"].as_u64().ok_or("模型内容编号无效")?;
                    let block = self
                        .blocks
                        .get_mut(&id)
                        .filter(|b| !b.closed)
                        .ok_or("模型内容块不存在或已结束")?;
                    let delta = &event["delta"];
                    let (field, part) = match delta["type"].as_str() {
                        Some("text_delta") if block.value["type"] == "text" => {
                            ("text", delta["text"].as_str())
                        }
                        Some("thinking_delta") if block.value["type"] == "thinking" => {
                            ("thinking", delta["thinking"].as_str())
                        }
                        Some("signature_delta") if block.value["type"] == "thinking" => {
                            ("signature", delta["signature"].as_str())
                        }
                        Some("input_json_delta") if block.value["type"] == "tool_use" => {
                            block
                                .json
                                .push_str(delta["partial_json"].as_str().ok_or("工具流参数无效")?);
                            continue;
                        }
                        _ => return Err("模型流内容类型不匹配".into()),
                    };
                    let part = part.ok_or("模型流内容格式无效")?;
                    let mut joined = block.value[field].as_str().unwrap_or("").to_owned();
                    joined.push_str(part);
                    block.value[field] = json!(joined);
                }
                Some("content_block_stop") => {
                    let id = event["index"].as_u64().ok_or("模型内容编号无效")?;
                    let block = self
                        .blocks
                        .get_mut(&id)
                        .filter(|b| !b.closed)
                        .ok_or("模型内容块结束状态无效")?;
                    if !block.json.is_empty() {
                        block.value["input"] = serde_json::from_str(&block.json)
                            .map_err(|_| "工具参数不是完整JSON")?;
                    }
                    block.closed = true;
                }
                Some("message_delta") => {
                    if !self.started || self.stop.is_some() {
                        return Err("模型结束顺序无效".into());
                    }
                    self.stop = Some(
                        event["delta"]["stop_reason"]
                            .as_str()
                            .ok_or("模型缺少结束标志")?
                            .to_string(),
                    );
                }
                Some("message_stop") => {
                    self.ended = true;
                    return self.complete().map(Some);
                }
                _ => return Err("模型流事件不受支持".into()),
            }
        }
        Ok(None)
    }
    fn complete(&self) -> Result<Vec<Value>, String> {
        if !self.started
            || self.blocks.values().any(|b| !b.closed)
            || !matches!(self.stop.as_deref(), Some("end_turn" | "tool_use"))
        {
            return Err("模型输出未完整结束，未执行后续操作".into());
        }
        let mut output = vec![];
        let mut ids = HashSet::new();
        for block in self.blocks.values() {
            let v = &block.value;
            match v["type"].as_str() {
                Some("thinking" | "redacted_thinking") => {
                    output.push(json!({"type":"reasoning","anthropic_blocks":[v]}))
                }
                Some("text") => {
                    if let Some(text) = v["text"].as_str().filter(|s| !s.is_empty()) {
                        output.push(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}));
                    }
                }
                Some("tool_use") => {
                    let id = v["id"]
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 512)
                        .ok_or("工具调用ID无效")?;
                    let name = v["name"]
                        .as_str()
                        .filter(|s| {
                            !s.is_empty()
                                && s.len() <= 128
                                && s.bytes()
                                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                        })
                        .ok_or("工具名称无效")?;
                    if !v["input"].is_object() || !ids.insert(id) || ids.len() > 16 {
                        return Err("工具调用不完整或重复".into());
                    }
                    output.push(json!({"type":"function_call","call_id":id,"name":name,"arguments":v["input"].to_string()}));
                }
                _ => return Err("模型内容类型无效".into()),
            }
        }
        if (self.stop.as_deref() == Some("tool_use")) == ids.is_empty() {
            return Err("工具结束标志与内容不一致".into());
        }
        for item in &mut output {
            if item["type"] != "reasoning" {
                item["provider_native"] = json!(true);
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(v: Value) -> Vec<u8> {
        format!("data: {}\n\n", v).into_bytes()
    }
    fn events(stop: &str) -> Vec<Value> {
        vec![
            json!({"type":"message_start","message":{"role":"assistant","id":"msg-test"}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"opaque model state"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed-state"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call-one","name":"phone_observe","input":{}}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"fresh\":"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"true}"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":stop}}),
            json!({"type":"message_stop"}),
        ]
    }
    #[test]
    fn signed_thinking_tool_arguments_and_result_images_round_trip() {
        let bytes: Vec<u8> = events("tool_use").into_iter().flat_map(frame).collect();
        let mut parser = MessagesStream::default();
        let mut output = None;
        for byte in bytes {
            let next = parser.feed(&[byte]).unwrap();
            if next.is_some() {
                output = next;
            }
        }
        let output = output.unwrap();
        assert_eq!(
            output[0]["anthropic_blocks"][0]["signature"],
            "signed-state"
        );
        assert!(output[0].get("summary").is_none());
        assert_eq!(output[1]["arguments"], "{\"fresh\":true}");
        let mut context =
            vec![json!({"role":"user","content":[{"type":"input_text","text":"Inspect"}]})];
        context.extend(output);
        context.push(json!({"type":"function_call_output","call_id":"call-one","output":[{"type":"input_text","text":"observed"},{"type":"input_image","image_url":"data:image/png;base64,aW1hZ2U="}]}));
        let mapped=request_body(&json!({"input":context,"instructions":"runtime policy","model":"test","max_output_tokens":256,"tools":[{"type":"function","name":"phone_observe","description":"Read","parameters":{"type":"object"}}],"tool_choice":{"name":"phone_observe"}})).unwrap();
        assert_eq!(mapped["system"], "runtime policy");
        assert_eq!(mapped["max_tokens"], 256);
        assert_eq!(
            mapped["messages"][1]["content"][0]["signature"],
            "signed-state"
        );
        assert_eq!(mapped["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(
            mapped["messages"][2]["content"][0]["tool_use_id"],
            "call-one"
        );
        assert_eq!(
            mapped["messages"][2]["content"][0]["content"][1]["source"]["type"],
            "base64"
        );
        assert_eq!(mapped["tool_choice"]["type"], "tool");
        assert_eq!(mapped["tools"][0]["input_schema"]["type"], "object");
        assert!(mapped.get("store").is_none());
    }
    #[test]
    fn truncated_or_mismatched_streams_never_release_actions() {
        for stop in ["max_tokens", "end_turn", "refusal"] {
            let mut p = MessagesStream::default();
            let mut error = false;
            for v in events(stop) {
                if p.feed(&frame(v)).is_err() {
                    error = true;
                    break;
                }
            }
            assert!(error, "{stop}");
        }
        let mut p = MessagesStream::default();
        for v in events("tool_use").into_iter().take(9) {
            assert!(p.feed(&frame(v)).unwrap().is_none());
        }
        // EOF at this point must never yield an executable tool batch.
        let mut invalid = events("tool_use");
        invalid[8] = json!({"type":"content_block_stop","index":99});
        let mut p = MessagesStream::default();
        assert!(invalid.into_iter().any(|v| p.feed(&frame(v)).is_err()));
    }
    #[test]
    fn runtime_checkpoints_keep_their_position() {
        let request=request_body(&json!({"input":[{"role":"user","content":"old"},{"role":"system","content":"new task begins"},{"role":"user","content":"current"}],"model":"test"})).unwrap();
        let parts = request["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "old");
        assert_eq!(parts[2]["text"], "new task begins");
        assert_eq!(parts[3]["text"], "current");
    }
}
