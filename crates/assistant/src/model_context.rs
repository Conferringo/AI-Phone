use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub fn for_chat_api(context: &[Value]) -> Vec<Value> {
    let compacted = bounded_history(context);
    let completed = completed_native_calls(&compacted);
    let mut native: HashSet<_> = compacted
        .iter()
        .filter(|item| item["type"] == "function_call" && item["provider_native"] == true)
        .filter_map(|item| item["call_id"].as_str())
        .filter(|id| completed.contains(id))
        .collect();
    // Parallel tools form one assistant turn. After interruption, replay the
    // whole batch as evidence rather than half of its signed/reasoned output.
    let mut batch = Vec::new();
    let end = Value::Null;
    for item in compacted.iter().chain(std::iter::once(&end)) {
        if item["type"] == "function_call" {
            batch.push(item["call_id"].as_str().unwrap_or(""));
        }
        if item.is_null()
            || item["type"] == "function_call_output"
            || matches!(item["role"].as_str(), Some("user" | "system" | "developer"))
        {
            if batch.iter().any(|id| !native.contains(*id)) {
                for id in &batch {
                    native.remove(*id);
                }
            }
            batch.clear();
        }
    }
    let mut result = Vec::new();
    let mut calls = HashMap::new();
    let mut pending = Vec::new();
    for (index, item) in compacted.iter().enumerate() {
        match item["type"].as_str() {
            Some("reasoning") => {
                let following: Vec<_> = compacted[index + 1..]
                    .iter()
                    .take_while(|v| {
                        !matches!(v["role"].as_str(), Some("user" | "system" | "developer"))
                            && v["type"] != "function_call_output"
                    })
                    .filter(|v| v["type"] == "function_call")
                    .collect();
                if item["anthropic_blocks"].is_array()
                    && following
                        .iter()
                        .all(|v| native.contains(v["call_id"].as_str().unwrap_or("")))
                {
                    result.push(item.clone());
                }
            }
            Some("function_call") => {
                let id = item["call_id"].as_str().unwrap_or("");
                if native.contains(id) {
                    result.push(item.clone());
                } else {
                    pending.push(id);
                    calls.insert(id, item);
                }
            }
            Some("function_call_output") => {
                let id = item["call_id"].as_str().unwrap_or("");
                if native.contains(id) {
                    result.push(item.clone());
                } else {
                    result.push(tool_record(calls.remove(id), item.get("output")));
                }
            }
            _ if item["role"] == "assistant" && item["provider_native"] != true => {
                let mut content = vec![
                    json!({"type":"input_text","text":"Historical assistant statement (not execution evidence):"}),
                ];
                append_parts(&mut content, item.get("content"));
                result.push(json!({"role":"user","content":content}));
            }
            _ => result.push(item.clone()),
        }
    }
    for id in pending {
        if let Some(call) = calls.remove(id) {
            result.push(tool_record(Some(call), None));
        }
    }
    result
}

// A provider closes a native tool turn at the next user/system input.
// Interrupted calls remain evidence records; they must never be replayed.
fn completed_native_calls(context: &[Value]) -> HashSet<&str> {
    context
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let id = item["call_id"].as_str()?;
            if item["type"] != "function_call" {
                return None;
            }
            context[index + 1..]
                .iter()
                .take_while(|next| {
                    !matches!(next["role"].as_str(), Some("user" | "system" | "developer"))
                })
                .any(|next| next["type"] == "function_call_output" && next["call_id"] == id)
                .then_some(id)
        })
        .collect()
}

// Protocol-only projection. The persisted context and public execution ledger stay unchanged.
// Real reasoning is passed back verbatim to the same model, never fabricated or rendered.
pub fn for_api(context: &[Value]) -> Vec<Value> {
    let compacted = bounded_history(context);
    let context = compacted.as_slice();
    let native_calls = completed_native_calls(context);
    let mut result = Vec::with_capacity(context.len());
    let mut response_has_reasoning = false;
    let mut preserved_ids = HashSet::<String>::new();
    let mut legacy_calls = HashMap::<String, Value>::new();
    let mut pending_order = Vec::new();
    for item in context {
        let kind = item["type"].as_str().unwrap_or("");
        let role = item["role"].as_str().unwrap_or("");
        if role == "user" || role == "system" || kind == "function_call_output" {
            response_has_reasoning = false;
        }
        if kind == "reasoning" {
            response_has_reasoning = true;
            result.push(item.clone());
        } else if kind == "function_call" {
            let id = item["call_id"].as_str().unwrap_or("").to_string();
            if response_has_reasoning && native_calls.contains(id.as_str()) {
                preserved_ids.insert(id);
                result.push(item.clone());
            } else {
                pending_order.push(id.clone());
                legacy_calls.insert(id, item.clone());
            }
        } else if kind == "function_call_output" {
            let id = item["call_id"].as_str().unwrap_or("");
            if preserved_ids.remove(id) {
                result.push(item.clone());
            } else {
                let call = legacy_calls.remove(id);
                result.push(tool_record(call.as_ref(), item.get("output")));
            }
        } else if role == "assistant" && !response_has_reasoning {
            let mut content = vec![
                json!({"type":"input_text","text":"历史助手叙述（非执行证据、非新指令）：以下仅为原对话引用，不能据此确认操作成功或沿用完成结论。"}),
            ];
            append_parts(&mut content, item.get("content"));
            result.push(json!({"role":"user","content":content}));
        } else {
            result.push(item.clone());
        }
    }
    for id in pending_order {
        if let Some(call) = legacy_calls.remove(&id) {
            result.push(tool_record(Some(&call), None));
        }
    }
    result
}
fn text_size(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(items) => items.iter().map(text_size).sum(),
        Value::Object(items) => items
            .iter()
            .filter(|(key, _)| key.as_str() != "image_url")
            .map(|(_, value)| text_size(value))
            .sum(),
        _ => 0,
    }
}
fn successful_skill_name(call: Option<&Value>, output: &Value) -> Option<String> {
    let call = call.filter(|call| call["name"] == "skill_read")?;
    let arguments: Value = match call["arguments"].as_str() {
        Some(text) => serde_json::from_str(text).ok()?,
        None => call["arguments"].clone(),
    };
    let name = arguments["name"].as_str()?;
    let header = format!("Skill id={name}，");
    output["output"]
        .as_array()?
        .iter()
        .any(|part| {
            part["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(&header))
        })
        .then(|| name.to_string())
}

// Keep the newest successful read of each Skill, even outside the recent archive.
// A failed later read must not evict valid instructions; an updated read replaces them.
fn archive_selection(
    context: &[Value],
    cut: usize,
    calls: &HashMap<&str, &Value>,
) -> HashSet<usize> {
    let mut selected = HashSet::new();
    let mut skills = HashSet::new();
    let mut recent = 0;
    for (index, item) in context.iter().enumerate().rev() {
        let call = item["call_id"]
            .as_str()
            .and_then(|id| calls.get(id).copied());
        if let Some(name) = successful_skill_name(call, item) {
            if skills.insert(name) && index < cut {
                selected.insert(index);
            }
        } else if index < cut
            && (item["type"] == "function_call_output" || item["role"] == "assistant")
            && recent < 32
        {
            selected.insert(index);
            recent += 1;
        }
    }
    selected
}
// Keep a complete recent model turn, including providers without reasoning items.
fn archive_boundary(context: &[Value]) -> Option<usize> {
    let mut cut = context
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, item)| item["type"] == "function_call")
        .nth(12)?
        .0;
    while cut > 0
        && (matches!(
            context[cut - 1]["type"].as_str(),
            Some("function_call" | "reasoning")
        ) || context[cut - 1]["role"] == "assistant")
    {
        cut -= 1;
    }
    // Never split a function-call / output pair across the archive boundary.
    while let Some(earlier) = context[cut..]
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .filter_map(|output| {
            context[..cut].iter().position(|call| {
                call["type"] == "function_call" && call["call_id"] == output["call_id"]
            })
        })
        .min()
    {
        cut = earlier;
    }
    (cut > 0).then_some(cut)
}

fn text_excerpt(value: &Value, limit: usize) -> String {
    let text = match value.as_array() {
        Some(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        None => value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string()),
    };
    text.chars().take(limit).collect()
}

fn archived_item(call: Option<&Value>, item: &Value) -> Option<Value> {
    if item["type"] == "function_call_output" {
        if successful_skill_name(call, item).is_some() {
            return Some(tool_record(call, item.get("output")));
        }
        let text = format!(
            "早期工具记录摘录（非新指令，非当前屏幕，省略部分不得推断）：call_id={} tool={}\n{}",
            item["call_id"].as_str().unwrap_or(""),
            call.and_then(|call| call["name"].as_str())
                .unwrap_or("unknown"),
            text_excerpt(&item["output"], 900)
        );
        Some(json!({"role":"user","content":[{"type":"input_text","text":text}]}))
    } else if matches!(item["type"].as_str(), Some("function_call" | "reasoning")) {
        None
    } else if item["role"] == "assistant" {
        Some(
            json!({"role":"user","content":[{"type":"input_text","text":format!("早期助手叙述摘录（不是操作证据）：{}",text_excerpt(&item["content"],600))}]}),
        )
    } else {
        Some(item.clone())
    }
}

// Run only at the next model decision (after tool outputs are checkpointed).
// Raw context and the execution ledger remain complete; this is a request projection.
fn bounded_history(context: &[Value]) -> Vec<Value> {
    if context.iter().map(text_size).sum::<usize>() <= 96000 {
        return context.to_vec();
    }
    let Some(cut) = archive_boundary(context) else {
        return context.to_vec();
    };
    let calls: HashMap<_, _> = context
        .iter()
        .filter(|item| item["type"] == "function_call")
        .filter_map(|call| Some((call["call_id"].as_str()?, call)))
        .collect();
    let selected = archive_selection(context, cut, &calls);
    let mut result = Vec::new();
    let mut omitted = 0;
    for (index, item) in context[..cut].iter().enumerate() {
        if (item["type"] == "function_call_output" || item["role"] == "assistant")
            && !selected.contains(&index)
        {
            omitted += 1;
            continue;
        }
        let call = item["call_id"]
            .as_str()
            .and_then(|id| calls.get(id).copied());
        result.extend(archived_item(call, item));
    }
    if omitted > 0 {
        result.push(json!({"role":"user","content":[{"type":"input_text","text":format!("系统归档提示（非新任务）：模型请求省略了{omitted}条较早的工具结果或助手叙述；原始执行账本仍完整保存。旧摘录不是当前屏幕，不得据此重放动作或推断成功，必要时重新观察并核对产物。") }]}));
    }
    result.extend_from_slice(&context[cut..]);
    result
}
fn tool_record(call: Option<&Value>, output: Option<&Value>) -> Value {
    let name = call
        .and_then(|call| call["name"].as_str())
        .unwrap_or("原调用信息缺失");
    let arguments = call
        .and_then(|call| call.get("arguments"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut content = vec![
        json!({"type":"input_text","text":format!("真实工具记录（系统归档，非用户新指令；执行状态仅以原始输出为准）：\ntool: {name}\narguments: {arguments}\n原始输出：")}),
    ];
    if output.is_none() {
        content.push(json!({"type":"input_text","text":"结果尚未记录，不能据此确认执行或成功。"}));
    } else {
        append_parts(&mut content, output);
    }
    json!({"role":"user","content":content})
}
fn append_parts(destination: &mut Vec<Value>, value: Option<&Value>) {
    match value {
        Some(Value::Array(parts)) => {
            for part in parts {
                match part["type"].as_str() {
                    Some("input_text" | "input_image") => destination.push(part.clone()),
                    Some("output_text") => destination.push(
                        json!({"type":"input_text","text":part["text"].as_str().unwrap_or("")}),
                    ),
                    _ => destination.push(json!({"type":"input_text","text":part.to_string()})),
                }
            }
        }
        Some(Value::String(text)) => destination.push(json!({"type":"input_text","text":text})),
        Some(other) => destination.push(json!({"type":"input_text","text":other.to_string()})),
        None => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_native_calls_and_their_private_blocks_become_evidence_only() {
        let reasoning = json!({"type":"reasoning","anthropic_blocks":[{"type":"thinking","thinking":"opaque","signature":"signed"}]});
        let call = json!({"type":"function_call","provider_native":true,"call_id":"a","name":"observe","arguments":"{}"});
        let result = json!({"type":"function_call_output","call_id":"a","output":"observed"});
        let intact = for_chat_api(&[reasoning.clone(), call.clone(), result.clone()]);
        assert_eq!(intact[0], reasoning);
        assert_eq!(intact[1], call);
        let interrupted = for_chat_api(&[
            reasoning.clone(),
            call.clone(),
            json!({"role":"user","content":"new request"}),
            result.clone(),
        ]);
        assert!(interrupted.iter().all(|v| v["type"] != "reasoning"
            && v["type"] != "function_call"
            && v["type"] != "function_call_output"));
        let mut legacy = call;
        legacy.as_object_mut().unwrap().remove("provider_native");
        let projected = for_chat_api(&[reasoning, legacy, result]);
        assert!(
            projected
                .iter()
                .all(|v| v["type"] != "reasoning" && v["type"] != "function_call")
        );
        assert!(
            serde_json::to_string(&projected)
                .unwrap()
                .contains("observed")
        );
    }
    #[test]
    fn an_interrupted_parallel_batch_does_not_replay_half_a_signed_turn() {
        let context = vec![
            json!({"type":"reasoning","anthropic_blocks":[{"type":"thinking","thinking":"private","signature":"signed"}]}),
            json!({"type":"function_call","provider_native":true,"call_id":"a","name":"observe","arguments":"{}"}),
            json!({"type":"function_call","provider_native":true,"call_id":"b","name":"inspect","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"a","output":"observed"}),
            json!({"role":"user","content":"interrupt"}),
        ];
        let projected = for_chat_api(&context);
        assert!(projected.iter().all(|v| v["type"] != "reasoning"
            && v["type"] != "function_call"
            && v["type"] != "function_call_output"));
        assert!(
            serde_json::to_string(&projected)
                .unwrap()
                .contains("observed")
        );
    }
    #[test]
    fn long_exploration_bounds_old_tool_material_without_losing_owner_constraints_or_skills() {
        let owner = json!({"role":"user","content":[{"type":"input_text","text":"Keep my existing accounts and choose your own activity."}]});
        let mut context = vec![owner.clone()];
        for version in 1..=2 {
            context.push(json!({"type":"function_call","name":"skill_read","call_id":format!("skill-{version}"),"arguments":"{\"name\":\"phone-use\"}"}));
            context.push(json!({"type":"function_call_output","call_id":format!("skill-{version}"),"output":[{"type":"input_text","text":format!("Skill id=phone-use，version-{version}: {}", "valid instructions ".repeat(100))}]}));
        }
        for index in 0..1500 {
            context.extend([
                json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"fixture"}]}),
                json!({"type":"function_call","name":"phone_observe","call_id":format!("long-{index}"),"arguments":"{}"}),
                json!({"type":"function_call_output","call_id":format!("long-{index}"),"output":[{"type":"input_text","text":"fixture observation ".repeat(200)}]}),
            ]);
        }
        let saved = context.clone();
        let request = for_api(&context);
        assert!(
            request.iter().map(text_size).sum::<usize>() < 100_000,
            "tool archives must not grow with the lifetime of an exploration run"
        );
        assert!(request.contains(&owner));
        assert!(
            request
                .iter()
                .any(|item| item.to_string().contains("version-2"))
        );
        assert!(
            !request
                .iter()
                .any(|item| item.to_string().contains("version-1"))
        );
        assert!(
            request
                .iter()
                .any(|item| item["type"] == "function_call" && item["call_id"] == "long-1499")
        );
        assert!(
            request.iter().any(
                |item| item["type"] == "function_call_output" && item["call_id"] == "long-1499"
            )
        );
        assert_eq!(
            context, saved,
            "projection cannot delete the actual execution history"
        );
    }
    #[test]
    fn non_thinking_history_compacts_and_a_failed_skill_read_keeps_valid_instructions() {
        let mut context = vec![
            json!({"role":"system","content":[{"type":"input_text","text":"Preserve owner scope"}]}),
            json!({"type":"function_call","name":"skill_read","call_id":"valid-skill","arguments":{"name":"phone-use"}}),
            json!({"type":"function_call_output","call_id":"valid-skill","output":[{"type":"input_text","text":"Skill id=phone-use，enabled=true\nValid instructions"}]}),
            json!({"type":"function_call","name":"skill_read","call_id":"failed-skill","arguments":"{\"name\":\"phone-use\"}"}),
            json!({"type":"function_call_output","call_id":"failed-skill","output":[{"type":"input_text","text":"Skill temporarily unavailable"}]}),
        ];
        for index in 0..1500 {
            context.extend([
                json!({"type":"function_call","name":"phone_observe","call_id":format!("plain-{index}"),"arguments":"{}"}),
                json!({"type":"function_call_output","call_id":format!("plain-{index}"),"output":[{"type":"input_text","text":format!("observation-{index}: {}", "fixture observation ".repeat(200))}]}),
            ]);
        }
        for request in [for_api(&context), for_chat_api(&context)] {
            assert!(request.iter().map(text_size).sum::<usize>() < 100_000);
            assert_eq!(request[0], context[0]);
            assert!(
                request
                    .iter()
                    .any(|item| item.to_string().contains("Valid instructions"))
            );
            assert!(
                request
                    .iter()
                    .any(|item| item.to_string().contains("observation-1499:"))
            );
            assert!(
                request
                    .iter()
                    .any(|item| item.to_string().contains("系统归档提示"))
            );
        }
    }
    #[test]
    fn steering_between_call_and_result_archives_only_the_interrupted_turn() {
        let original = vec![
            json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"actual provider reasoning"}]}),
            json!({"type":"function_call","call_id":"interrupted","name":"knowledge_search","arguments":"{}"}),
            json!({"role":"system","content":[{"type":"input_text","text":"New steering; previous action was interrupted"}]}),
            json!({"role":"user","content":[{"type":"input_text","text":"Continue with a different task"}]}),
            json!({"type":"function_call_output","call_id":"interrupted","output":"not_executed"}),
            json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"next real reasoning"}]}),
            json!({"type":"function_call","call_id":"complete","name":"phone_observe","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"complete","output":"actual observation"}),
        ];
        let saved = original.clone();
        let projected = for_api(&original);
        assert_eq!(original, saved);
        assert!(
            !projected
                .iter()
                .any(|item| item["call_id"] == "interrupted")
        );
        assert!(
            projected
                .iter()
                .any(|item| item["call_id"] == "complete" && item["type"] == "function_call")
        );
        assert!(projected.iter().any(|item| item["call_id"] == "complete" && item["type"] == "function_call_output"));
        assert!(
            projected
                .iter()
                .any(|item| item.to_string().contains("not_executed"))
        );
        assert_eq!(projected[1], saved[2]);
    }
    #[test]
    fn large_history_compacts_old_material_and_keeps_recent_tool_pairs() {
        let mut original = vec![
            json!({"role":"system","content":[{"type":"input_text","text":"约束不变"}]}),
            json!({"role":"user","content":[{"type":"input_text","text":"用户目标不变"}]}),
        ];
        for i in 0..40 {
            original.extend([json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"fixture"}]}),json!({"type":"function_call","name":"phone_observe","call_id":format!("c{i}"),"arguments":"{}"}),json!({"type":"function_call_output","call_id":format!("c{i}"),"output":[{"type":"input_text","text":"test output ".repeat(1000)}]})]);
        }
        let saved = original.clone();
        let projected = for_api(&original);
        assert_eq!(original, saved);
        assert_eq!(projected[0], original[0]);
        assert_eq!(projected[1], original[1]);
        assert!(
            projected.iter().map(text_size).sum::<usize>()
                < original.iter().map(text_size).sum::<usize>() / 2
        );
        for call in projected.iter().filter(|v| v["type"] == "function_call") {
            assert!(projected.iter().any(|v|v["type"]=="function_call_output"&&v["call_id"]==call["call_id"]));
        }
        assert_eq!(projected.last(), original.last());
    }
    #[test]
    fn engine_skill_and_legacy_tool_outputs_become_quoted_user_records() {
        let original = vec![
            json!({"role":"user","content":[{"type":"input_text","text":"打开设置"}]}),
            json!({"type":"function_call","name":"skill_read","call_id":"engine-skill","arguments":"{\"name\":\"phone-use\"}"}),
            json!({"type":"function_call_output","call_id":"engine-skill","output":[{"type":"input_text","text":"Skill id=phone-use，真实正文"},{"type":"input_image","image_url":"data:image/png;base64,fixture"}]}),
        ];
        let saved = original.clone();
        let projected = for_api(&original);
        assert_eq!(original, saved);
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[1]["role"], "user");
        assert!(
            projected[1]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("skill_read")
        );
        assert_eq!(projected[1]["content"][1], original[2]["output"][0]);
        assert_eq!(projected[1]["content"][2], original[2]["output"][1]);
        assert!(
            !projected
                .iter()
                .any(|item| item["type"] == "reasoning" || item["type"] == "function_call")
        );
    }
    #[test]
    fn genuine_thinking_batch_keeps_both_call_output_pairs_and_assistant() {
        let original = vec![
            json!({"type":"reasoning","id":"reasoning-fixture","content":[{"type":"reasoning_text","text":"Protocol test fixture only"}]}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"先观察真实状态。"}]}),
            json!({"type":"function_call","name":"phone_observe","call_id":"first","arguments":"{}"}),
            json!({"type":"function_call","name":"skill_list","call_id":"second","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"first","output":[{"type":"input_image","image_url":"data:image/png;base64,fixture"}]}),
            json!({"type":"function_call_output","call_id":"second","output":"[]"}),
        ];
        assert_eq!(for_api(&original), original);
    }
    #[test]
    fn old_assistant_is_not_execution_evidence_and_reasoning_scope_resets() {
        let original = vec![
            json!({"role":"assistant","content":[{"type":"output_text","text":"我已经安装好了。"}]}),
            json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"Fixture"}]}),
            json!({"role":"system","content":[{"type":"input_text","text":"新的验证要求"}]}),
            json!({"type":"function_call","name":"phone_tap","call_id":"legacy","arguments":"{\"x\":1,\"y\":2}"}),
            json!({"type":"function_call_output","call_id":"legacy","output":"{\"success\":false}"}),
            json!({"role":"assistant","content":[{"type":"output_text","text":"不能确认已执行。"}]}),
        ];
        let projected = for_api(&original);
        assert_eq!(projected[0]["role"], "user");
        assert!(
            projected[0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("非执行证据")
        );
        assert_eq!(projected[0]["content"][1]["text"], "我已经安装好了。");
        assert!(
            !projected
                .iter()
                .any(|item| item["type"] == "function_call"
                    || item["type"] == "function_call_output")
        );
        assert_eq!(projected.last().unwrap()["role"], "user");
    }
}
