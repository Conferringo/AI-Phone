use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SLICE_TURNS: i32 = 8;
pub const ARCHIVED_OBSERVATION: &str = "AI_PHONE_VALIDATED_OBSERVATION_ARCHIVED:";
#[derive(Clone, Serialize, Deserialize)]
pub struct ExplorationWindow {
    first_step: i32,
    started_at_ms: u64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoopState {
    pub stalls: u32,
    pub observation_recoveries: u32,
    pub model_retries: u32,
    pub model_replay_boundary: usize,
    pub recent_results: Vec<String>,
    pub last_issue: String,
    pub plan: Option<Value>,
    pub exploration_window: Option<ExplorationWindow>,
}
impl LoopState {
    pub fn exploration_yield_due(&mut self, now: u64, step: i32) -> bool {
        let window = self.exploration_window.get_or_insert(ExplorationWindow {
            first_step: step,
            started_at_ms: now,
        });
        if step.saturating_sub(window.first_step) < 40
            && now.saturating_sub(window.started_at_ms) < 600_000
        {
            return false;
        }
        // Start the next window on wake, not before its potentially long wait.
        self.exploration_window = None;
        true
    }
    pub fn no_progress(&mut self, reason: &str) -> bool {
        self.stalls = self.stalls.saturating_add(1);
        self.last_issue = reason.chars().take(1000).collect();
        self.stalls >= 3
    }
    pub fn tool_result(
        &mut self,
        name: &str,
        arguments: &str,
        result: &Value,
        failed: bool,
    ) -> bool {
        if name == "task_wait" {
            return false;
        }
        if name == "task_history" {
            return self.no_progress("已核对执行记录，但尚未新增目标执行结果");
        }
        if name == "task_plan" {
            return if self.plan.is_some() {
                self.no_progress("重复调整计划，但没有新增执行结果")
            } else {
                false
            };
        }
        if failed {
            return self.no_progress(&format!(
                "工具{name}失败：{}",
                result.to_string().chars().take(500).collect::<String>()
            ));
        }
        let signature = format!(
            "{:x}",
            Sha256::digest(format!("{name}\n{arguments}\n{result}").as_bytes())
        );
        if self.recent_results.contains(&signature) {
            self.no_progress("重复取得相同工具结果，没有观察到新进展")
        } else {
            self.stalls = 0;
            self.last_issue.clear();
            self.recent_results.push(signature);
            if self.recent_results.len() > 24 {
                self.recent_results.remove(0);
            }
            false
        }
    }
}
pub fn slice_due(start_step: i32, step: i32) -> bool {
    step.saturating_sub(start_step) >= SLICE_TURNS
}
pub fn model_retry_delay(error: &str, attempt: u32) -> Option<u64> {
    let transient = [
        "HTTP 429",
        "HTTP 500",
        "HTTP 502",
        "HTTP 503",
        "HTTP 504",
        "网络连接失败",
        "模型流中断",
        "模型未返回完整结果",
        "模型未完成本轮回复",
    ]
    .iter()
    .any(|text| error.contains(text));
    if !transient || attempt > 8 {
        return None;
    }
    Some((2000 * 2_u64.pow(attempt.saturating_sub(1).min(5))).min(60000))
}
pub fn wake_at(now_ms: u64, seconds: u64) -> Result<u64, String> {
    if !(1..=300).contains(&seconds) {
        return Err("等待时间必须为1–300秒".into());
    }
    Ok(now_ms.saturating_add(seconds * 1000))
}
pub fn compact_observations(context: &mut [Value]) {
    let mut keep = std::collections::HashMap::<String, usize>::new();
    for item in context.iter_mut().rev() {
        if item["type"] != "function_call_output" {
            continue;
        }
        let Some(parts) = item["output"].as_array_mut() else {
            continue;
        };
        if !parts.iter().any(|part| part["type"] == "input_image") {
            continue;
        }
        let device = parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .find_map(|text| {
                let data: Value = serde_json::from_str(text).ok()?;
                if data["observation_id"].is_string() {
                    data["device_id"].as_str().map(str::to_string)
                } else {
                    None
                }
            })
            .or_else(|| {
                parts
                    .iter()
                    .any(|p| {
                        p["text"]
                            .as_str()
                            .is_some_and(|t| t.starts_with("真实手机屏幕 "))
                    })
                    .then(|| "phone-local-01".into())
            });
        let Some(device) = device else {
            continue;
        };
        let left = keep.entry(device).or_insert(2);
        if *left > 0 {
            *left -= 1;
            continue;
        }
        for part in parts.iter_mut() {
            if part["type"] == "input_image" {
                let digest = format!(
                    "{:x}",
                    Sha256::digest(part["image_url"].as_str().unwrap_or("").as_bytes())
                );
                *part = serde_json::json!({"type":"input_text","text":format!("{ARCHIVED_OBSERVATION}{digest}（本轮已验证观察；原图保存在执行记录）")});
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exploration_windows_preserve_activity_and_errors_across_restart_and_long_waits() {
        let mut state = LoopState {
            plan: Some(json!({"goal":"self-chosen"})),
            ..Default::default()
        };
        state.no_progress("actual failure");
        assert!(!state.exploration_yield_due(1000, 100));
        let mut resumed: LoopState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert!(!resumed.exploration_yield_due(2000, 139));
        assert!(resumed.exploration_yield_due(3000, 140));
        assert_eq!(resumed.plan, state.plan);
        assert_eq!(resumed.stalls, 1);
        let mut resumed: LoopState =
            serde_json::from_value(serde_json::to_value(&resumed).unwrap()).unwrap();
        assert!(!resumed.exploration_yield_due(3_600_000, 140));
        assert!(resumed.exploration_yield_due(4_200_000, 141));
        let legacy: LoopState = serde_json::from_value(json!({"stalls":2})).unwrap();
        assert!(legacy.exploration_window.is_none());
        assert_eq!(legacy.stalls, 2);
    }
    #[test]
    fn transient_model_errors_retry_without_replaying_tools() {
        assert_eq!(model_retry_delay("模型服务返回 HTTP 429", 1), Some(2000));
        assert_eq!(model_retry_delay("模型服务返回 HTTP 503", 8), Some(60000));
        assert_eq!(model_retry_delay("模型服务返回 HTTP 429", 9), None);
        assert_eq!(model_retry_delay("模型服务返回 HTTP 401", 1), None);
        assert_eq!(model_retry_delay("模型额度已耗尽", 1), None);
    }
    #[test]
    fn more_than_forty_turns_continue_in_bounded_slices() {
        let mut step = 0;
        let mut slices = 0;
        while step < 56 {
            let start = step;
            while !slice_due(start, step) {
                step += 1;
            }
            slices += 1;
        }
        assert_eq!((step, slices), (56, 7));
    }
    #[test]
    fn persisted_wait_keeps_its_original_deadline_after_restart() {
        let deadline = wake_at(1000, 120).unwrap();
        let saved = json!({"wake_at_ms":deadline});
        let loaded: Value = serde_json::from_str(&saved.to_string()).unwrap();
        assert_eq!(loaded["wake_at_ms"], 121000);
        assert!(60000 < loaded["wake_at_ms"].as_u64().unwrap());
        assert!(122000 >= loaded["wake_at_ms"].as_u64().unwrap());
        assert!(wake_at(0, 0).is_err());
        assert!(wake_at(0, 301).is_err());
    }
    #[test]
    fn repeated_failures_and_stalls_survive_restart() {
        let mut state = LoopState::default();
        assert!(!state.no_progress("no result"));
        let mut restored: LoopState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert!(!restored.no_progress("same error"));
        assert!(restored.no_progress("same error"));
    }
    #[test]
    fn observation_compression_keeps_latest_two_and_validation_markers() {
        let mut context:Vec<_>=(0..5).map(|index|json!({"type":"function_call_output","call_id":index.to_string(),"output":[{"type":"input_text","text":"真实手机屏幕 720×1440"},{"type":"input_image","image_url":format!("data:image/png;base64,frame-{index}")}]})).collect();
        compact_observations(&mut context);
        let image_count = context
            .iter()
            .flat_map(|item| item["output"].as_array().unwrap())
            .filter(|part| part["type"] == "input_image")
            .count();
        assert_eq!(image_count, 2);
        assert!(
            context[0]["output"][1]["text"]
                .as_str()
                .unwrap()
                .starts_with(ARCHIVED_OBSERVATION)
        );
    }
}
