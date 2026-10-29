//! The execution engine owns evidence and tools; this module owns model transport.
use crate::{settings::ModelConfig, *};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Instant;

pub async fn request(
    config: &ModelConfig,
    mut body: Value,
    cancel: &CancellationToken,
) -> Result<Vec<Value>, String> {
    body["model"] = json!(config.model);
    body["stream"] = json!(true);
    body["store"] = json!(false);
    // DeepSeek-specific reasoning levels are not sent to arbitrary compatible endpoints.
    if config.provider != "deepseek" {
        body.as_object_mut()
            .ok_or("模型请求格式错误")?
            .remove("reasoning");
    }
    let chat_protocol = config.protocol == "chat-completions";
    if chat_protocol {
        body = crate::chat_completions::request_body(&body)?;
    }
    let client = config.client()?;
    let mut request = client
        .post(config.endpoint()?)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&body);
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }
    let response = tokio::select! {_ = cancel.cancelled()=>return Err("canceled".into()),r=request.send()=>r.map_err(|_|"模型网络连接失败，请检查地址、代理与服务状态")?};
    if !response.status().is_success() {
        // Inspect only bounded error codes; never expose provider messages or echoed input.
        let status = response.status().as_u16();
        if status == 400 {
            return Err(
                "模型拒绝请求（HTTP 400）；请核对协议、工具往返与上下文格式。配置和任务已保留"
                    .into(),
            );
        }
        if status == 429 {
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(Ok(chunk)) = tokio::select! {
                _ = cancel.cancelled() => return Err("canceled".into()),
                chunk = stream.next() => chunk,
            } {
                if bytes.len() + chunk.len() > 8192 {
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            if serde_json::from_slice::<Value>(&bytes)
                .ok()
                .is_some_and(|value| {
                    ["insufficient_quota", "billing_hard_limit_reached"]
                        .contains(&value["error"]["code"].as_str().unwrap_or(""))
                })
            {
                return Err("模型额度已耗尽，请检查服务商余额；任务和记录已保留".into());
            }
        }
        return Err(format!(
            "模型服务返回 HTTP {}；请检查模型 ID、密钥和服务权限",
            status
        ));
    }
    if !response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        })
    {
        return Err("模型未返回 text/event-stream；请检查 API 地址与协议".into());
    }
    let mut stream = response.bytes_stream();
    let mut parser = ResponseStream::default();
    let mut chat_parser = crate::chat_completions::ChatStream::default();
    loop {
        let chunk = tokio::select! {_ = cancel.cancelled()=>return Err("canceled".into()),chunk=stream.next()=>chunk};
        match chunk {
            Some(Ok(chunk)) => {
                if let Some(output) = if chat_protocol {
                    chat_parser.feed(&chunk)?
                } else {
                    parser.feed(&chunk)?
                } {
                    return Ok(output);
                }
            }
            Some(Err(_)) => return Err("模型流中断，尚未执行后续操作".into()),
            None => return Err("模型未返回完整结果，尚未执行后续操作".into()),
        }
    }
}
#[derive(Default)]
struct ResponseStream {
    sse: crate::model_sse::SseDecoder,
}
impl ResponseStream {
    fn feed(&mut self, chunk: &[u8]) -> Result<Option<Vec<Value>>, String> {
        for data in self.sse.feed(chunk)? {
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let event: Value = serde_json::from_str(&data).map_err(|_| "模型流格式无效")?;
            match event["type"].as_str() {
                Some("response.completed") => {
                    return event["response"]["output"]
                        .as_array()
                        .cloned()
                        .map(Some)
                        .ok_or("模型返回缺少完整输出".into());
                }
                Some("response.failed" | "response.incomplete" | "error") => {
                    return Err("模型未完成本轮回复；未将部分输出作为操作执行".into());
                }
                _ => {}
            }
        }
        Ok(None)
    }
}
fn text(output: &[Value]) -> String {
    output
        .iter()
        .filter(|v| v["type"] == "message")
        .flat_map(|v| v["content"].as_array().into_iter().flatten())
        .filter_map(|v| v["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}
async fn check(
    config: &ModelConfig,
    name: &str,
    body: Value,
    expected: &str,
) -> wire::CapabilityCheck {
    let start = Instant::now();
    let cancel = CancellationToken::new();
    let result =
        tokio::time::timeout(Duration::from_secs(55), request(config, body, &cancel)).await;
    let (passed, detail) = match result {
        Ok(Ok(output)) if text(&output).contains(expected) => (true, "通过".into()),
        Ok(Ok(_)) => (false, "已收到输出，但实际内容未通过验证".into()),
        Ok(Err(e)) => (false, e),
        Err(_) => (false, "测试超过 55 秒，请检查网络或模型速度".into()),
    };
    wire::CapabilityCheck {
        name: name.into(),
        passed,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
pub async fn probe(config: &ModelConfig) -> Vec<wire::CapabilityCheck> {
    let nonce = Uuid::new_v4().simple().to_string();
    let text_body = json!({"input":[{"role":"user","content":[{"type":"input_text","text":format!("Reply exactly with this code: {nonce}")}]}],"max_output_tokens":256,"reasoning":{"effort":"none"}});
    let image_body = json!({"input":[{"role":"user","content":[{"type":"input_text","text":"Read the four digits in this image. Reply only with the digits."},{"type":"input_image","image_url":format!("data:image/png;base64,{}",STANDARD.encode(include_bytes!("../fixtures/vision-probe.png")))}]}],"max_output_tokens":256,"reasoning":{"effort":"none"}});
    let (text_check, image_check, tool_check) = tokio::join!(
        check(config, "文字与流式输出", text_body, &nonce),
        check(config, "图片识别", image_body, "7319"),
        tool_probe(config)
    );
    vec![text_check, image_check, tool_check]
}
async fn tool_probe(config: &ModelConfig) -> wire::CapabilityCheck {
    let start = Instant::now();
    let nonce = Uuid::new_v4().simple().to_string();
    let cancel = CancellationToken::new();
    let tool = json!({"type":"function","name":"diagnostic_echo","description":"Returns a diagnostic result; no external effects.","parameters":{"type":"object","properties":{"code":{"type":"string"}},"required":["code"],"additionalProperties":false}});
    let initial = json!({"role":"user","content":[{"type":"input_text","text":format!("Call diagnostic_echo with code {nonce}. After the result, reply with only the returned receipt.")} ]});
    let work = async {
        let output=request(config,json!({"input":[initial.clone()],"tools":[tool],"tool_choice":{"type":"function","name":"diagnostic_echo"},"parallel_tool_calls":false,"reasoning":{"effort":"none"},"max_output_tokens":512}),&cancel).await?;
        let calls: Vec<_> = output
            .iter()
            .filter(|v| v["type"] == "function_call")
            .collect();
        if calls.len() != 1 || calls[0]["name"] != "diagnostic_echo" {
            return Err("模型没有正确调用测试工具".into());
        }
        let call = calls[0];
        let args: Value = serde_json::from_str(call["arguments"].as_str().unwrap_or(""))
            .map_err(|_| "工具参数不是有效 JSON")?;
        if args["code"] != nonce || call["call_id"].as_str().unwrap_or("").is_empty() {
            return Err("工具参数或调用标识不匹配".into());
        }
        let receipt = Uuid::new_v4().simple().to_string();
        let result = json!({"type":"function_call_output","call_id":call["call_id"],"output":json!({"receipt":receipt}).to_string()});
        let mut input = vec![initial];
        input.extend(output);
        input.push(result);
        let final_output = request(
            config,
            json!({"input":input,"max_output_tokens":256,"reasoning":{"effort":"none"}}),
            &cancel,
        )
        .await?;
        if !text(&final_output).contains(&receipt) {
            return Err("模型未读取工具的实际返回结果".into());
        }
        Ok::<(), String>(())
    };
    let (passed, detail) = match tokio::time::timeout(Duration::from_secs(110), work).await {
        Ok(Ok(())) => (true, "调用参数、结果回传与后续读取均通过".into()),
        Ok(Err(e)) => (false, e),
        Err(_) => (false, "工具往返测试超时".into()),
    };
    wire::CapabilityCheck {
        name: "工具调用与结果读取".into(),
        passed,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_rate_limit_body() {
        use axum::{Router, body::Body, http::StatusCode, routing::post};
        let ready = Arc::new(tokio::sync::Notify::new());
        let handler_ready = ready.clone();
        let router = Router::new().route(
            "/v1/responses",
            post(move || {
                let ready = handler_ready.clone();
                async move {
                    let chunks = futures_util::stream::once(async move {
                        ready.notify_one();
                        Ok::<_, std::io::Error>(Bytes::from_static(b"{"))
                    })
                    .chain(futures_util::stream::pending());
                    (StatusCode::TOO_MANY_REQUESTS, Body::from_stream(chunks))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}/v1"),
            model: "fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let mut task =
            tokio::spawn(async move { request(&config, json!({"input":[]}), &token).await });
        tokio::time::timeout(Duration::from_secs(2), ready.notified())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_millis(300), &mut task).await;
        task.abort();
        server.abort();
        assert_eq!(
            result
                .expect("cancel must not wait for the provider error body")
                .unwrap(),
            Err("canceled".into())
        );
    }
    #[tokio::test]
    async fn chat_protocol_round_trip_through_a_real_local_http_endpoint() {
        use axum::{Json, Router, routing::post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router=Router::new().route("/v1/chat/completions",post(|Json(value):Json<Value>| async move {
            assert_eq!(value["model"],"fixture-model");
            assert_eq!(value["stream"],true);
            assert!(value.get("input").is_none());
            let text=value["messages"].as_array().unwrap().iter().find(|m|m["role"]=="tool");
            let delta=if let Some(result)=text {
                assert_eq!(result["tool_call_id"],"fixture-call");
                json!({"content":format!("Receipt: {}",result["content"].as_str().unwrap())})
            } else {
                assert_eq!(value["messages"][0]["content"][1]["image_url"]["url"],"data:image/png;base64,fixture");
                json!({"tool_calls":[{"index":0,"id":"fixture-call","type":"function","function":{"name":"diagnostic_echo","arguments":"{\"code\":\"receipt\"}"}}]})
            };
            let reason=if text.is_some(){"stop"}else{"tool_calls"};
            ([ ("content-type","text/event-stream") ],format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"index":0,"delta":delta,"finish_reason":reason}]})))
        }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}/v1"),
            model: "fixture-model".into(),
            provider: "compatible".into(),
            protocol: "chat-completions".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let input = json!({"role":"user","content":[{"type":"input_text","text":"inspect"},{"type":"input_image","image_url":"data:image/png;base64,fixture"}]});
        let output = request(
            &config,
            json!({"input":[input.clone()]}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(output[0]["name"], "diagnostic_echo");
        let mut context = vec![input];
        context.extend(output);
        context.push(json!({"type":"function_call_output","call_id":"fixture-call","output":"actual-local-result"}));
        let output = request(&config, json!({"input":context}), &CancellationToken::new())
            .await
            .unwrap();
        assert!(text(&output).contains("actual-local-result"));
        server.abort();
    }
    #[test]
    fn stream_accepts_split_utf8_and_completed_without_done() {
        let mut p = ResponseStream::default();
        let bytes="data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"text\":\"成功\"}]}}\r\n\r\n".as_bytes();
        let cut = bytes.iter().position(|b| *b > 127).unwrap() + 1;
        assert!(p.feed(&bytes[..cut]).unwrap().is_none());
        assert_eq!(p.feed(&bytes[cut..]).unwrap().unwrap()[0]["text"], "成功");
    }
    #[test]
    fn responses_support_standard_multiline_event_data() {
        let mut parser = ResponseStream::default();
        let event = b": heartbeat\r\nevent: response.completed\r\ndata: {\"type\":\"response.completed\",\r\ndata: \"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"text\":\"receipt\"}]}]}}\r\n\r\n";
        let output = parser.feed(event).unwrap().unwrap();
        assert_eq!(output[0]["content"][0]["text"], "receipt");
    }
    #[tokio::test]
    async fn successful_http_with_a_non_stream_body_is_not_model_success() {
        use axum::{Router, routing::post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().route(
            "/responses",
            post(|| async {
                (
                    [("content-type", "application/json")],
                    "data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n",
                )
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}"),
            model: "fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let result = request(&config, json!({"input":[]}), &CancellationToken::new()).await;
        server.abort();
        assert!(result.unwrap_err().contains("text/event-stream"));
    }
    #[tokio::test]
    async fn full_endpoint_addresses_are_not_appended_twice() {
        use axum::{Router, routing::post};
        for (protocol, suffix, body) in [
            (
                "responses",
                "responses",
                "data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"text\":\"receipt\"}]}]}}\n\n",
            ),
            (
                "chat-completions",
                "chat/completions",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"receipt\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = Router::new().route(
                &format!("/v1/{suffix}"),
                post(move || async move {
                    ([("content-type", "text/event-stream; charset=utf-8")], body)
                }),
            );
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let config = ModelConfig {
                api_key: String::new(),
                base_url: format!("http://{address}/v1/{suffix}"),
                model: "fixture".into(),
                provider: "compatible".into(),
                protocol: protocol.into(),
                proxy_url: String::new(),
                verified_at_ms: 0,
            };
            let output = request(&config, json!({"input":[]}), &CancellationToken::new()).await;
            server.abort();
            assert_eq!(text(&output.unwrap()), "receipt");
        }
    }
    #[test]
    fn partial_and_failed_streams_never_produce_tools() {
        let mut p = ResponseStream::default();
        assert!(
            p.feed(
                b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\"}\n\n"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            p.feed(b"data: {\"type\":\"response.incomplete\"}\n\n")
                .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "real provider capability test; uses private local configuration, no phone or chat writes"]
    async fn live_model_capabilities() {
        let root = std::env::var("AI_PHONE_LIVE_TEST_ROOT").expect("explicit live test root");
        let store = crate::settings::SettingsStore::open(std::path::Path::new(&root)).unwrap();
        let config = store.get(store.current()).unwrap();
        let checks = probe(&config).await;
        for check in &checks {
            println!(
                "{}: {} ({}ms) {}",
                check.name, check.passed, check.duration_ms, check.detail
            );
        }
        assert!(
            checks.iter().all(|c| c.passed),
            "Live capability test failed"
        );
    }
}
