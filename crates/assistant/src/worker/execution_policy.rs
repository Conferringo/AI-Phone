//! Device interaction policy. Arbitrary commands require a user-owned purpose.
use super::{
    computer_task, evidence_start, explanatory_request, latest_request, maintenance_request,
    phone_task, successful_call, tool_output, tool_value,
};
use crate::tool_catalog::tools;
use serde_json::{Value, json};

pub const GUI_GUIDANCE: &str = "需要设备操作或联网检索时，必须在真实设备中完成：手机用phone-use，电脑用computer-use；搜索资料应打开设备浏览器，应用内搜索使用对应应用。不得用模型内置搜索、命令抓取或工作容器替代。workspace_read/write/list、知识库和Skill管理仍可使用。直接执行下一步；工具已返回动作后的新画面时不重复观察，不输出冗长引导。普通聊天不做无关操作。最终只报告用户目标与实际限制；后台学习由验收阶段处理，不自行声明是否已更新经验。";

pub fn command_tool(name: &str) -> bool {
    matches!(
        name,
        "computer_run" | "phone_shell" | "workspace_run" | "workspace_fetch"
    )
}

pub fn lookup_request(request: &str) -> bool {
    let text = request.to_lowercase();
    if [
        "只解释",
        "仅解释",
        "解释原理",
        "仅复述",
        "不要调用工具",
        "只要教程",
        "explain only",
        "do not use tools",
    ]
    .iter()
    .any(|word| text.contains(word))
    {
        return false;
    }
    let online = online_request(&text);
    if !online
        && [
            "知识库",
            "工作目录",
            "工作文件",
            "本地文件",
            "附件",
            "memory",
            "knowledge base",
        ]
        .iter()
        .any(|word| text.contains(word))
    {
        return false;
    }
    online
        || [
            "搜索",
            "检索",
            "查找",
            "查询",
            "查资料",
            "搜一下",
            "search",
            "research",
            "look up",
        ]
        .iter()
        .any(|word| text.contains(word))
        || (text.contains('找')
            && ["企业", "公司", "资料", "文章", "产品", "新闻", "数据"]
                .iter()
                .any(|word| text.contains(word)))
}

fn online_request(text: &str) -> bool {
    super::work_intent::positive_clauses(text).any(|clause| {
        [
            "浏览器",
            "官网",
            "网上",
            "联网",
            "互联网",
            "browser",
            "website",
            "online",
        ]
        .iter()
        .any(|word| clause.contains(word))
    })
}

pub fn browser_lookup(request: &str) -> bool {
    if !lookup_request(request) {
        return false;
    }
    let text = request.to_lowercase();
    online_request(&text)
        || (![
            "微信",
            "wechat",
            "小红书",
            "抖音",
            "bilibili",
            "mousepad",
            "文档中",
            "表格中",
        ]
        .iter()
        .any(|word| text.contains(word))
            && [
                "企业",
                "公司",
                "资料",
                "新闻",
                "产品",
                "论文",
                "行情",
                "research",
                "companies",
                "news",
                "web search",
            ]
            .iter()
            .any(|word| text.contains(word)))
}

pub fn visible_action(name: &str) -> bool {
    matches!(
        name,
        "computer_click"
            | "computer_type"
            | "computer_key"
            | "computer_scroll"
            | "computer_drag"
            | "phone_tap"
            | "phone_tap_element"
            | "phone_input"
            | "phone_key"
            | "phone_scroll"
            | "phone_swipe"
            | "phone_long_press"
            | "phone_drag"
            | "phone_gesture"
    )
}

pub fn browser_frame(value: &Value) -> bool {
    if !value["observation_id"].is_string() {
        return false;
    }
    let app = value["foreground_app"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    let title = value["title"].as_str().unwrap_or("").to_lowercase();
    [
        "org.cromite",
        "com.android.chrome",
        "org.mozilla",
        "com.microsoft.emmx",
        "com.opera",
        "com.brave.browser",
    ]
    .iter()
    .any(|name| app.contains(name))
        || [
            "chromium",
            "mozilla firefox",
            "firefox",
            "google chrome",
            "microsoft edge",
            "cromite",
            "brave",
            "vivaldi",
        ]
        .iter()
        .any(|name| {
            title == *name
                || title.strip_suffix(name).is_some_and(|prefix| {
                    [" - ", " — ", " – "]
                        .iter()
                        .any(|separator| prefix.ends_with(separator))
                })
        })
}

pub fn programming_request(request: &str) -> bool {
    super::work_intent::positive_clauses(&request.to_lowercase()).any(|clause| {
        [
            "运行脚本",
            "编写脚本",
            "写个脚本",
            "用python",
            "用 python",
            "用代码",
            "运行代码",
            "用node",
            "用 node",
            "用bun",
            "用 bun",
            "run a script",
            "write code",
            "run code",
        ]
        .iter()
        .any(|word| clause.contains(word))
    })
}

pub(super) fn command_execution(context: &[Value]) -> bool {
    let request = latest_request(context);
    !explanatory_request(&request)
        && !lookup_request(&request)
        && (maintenance_request(context) || programming_request(&request))
}

pub fn filter(mut tools: Value, restricted: bool) -> Value {
    if let Some(items) = tools.as_array_mut() {
        items.retain(|tool| {
            tool["type"] == "function"
                && (!restricted || !command_tool(tool["name"].as_str().unwrap_or("")))
        });
    }
    tools
}

pub(super) fn lookup_evidence(context: &[Value]) -> bool {
    let mut interacted = false;
    let browser = browser_lookup(&latest_request(context));
    for call in context.iter().skip(evidence_start(context)) {
        if !successful_call(call, context) {
            continue;
        }
        interacted |= visible_action(call["name"].as_str().unwrap_or(""));
        let name = call["name"].as_str().unwrap_or("");
        if interacted
            && (visible_action(name) || matches!(name, "computer_observe" | "phone_observe"))
            && tool_output(context, call)
                .and_then(Value::as_array)
                .is_some_and(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
                        .any(|value| {
                            value["observation_id"].is_string()
                                && (!browser || browser_frame(&value))
                        })
                })
        {
            return true;
        }
    }
    false
}

pub(super) fn gui_execution(context: &[Value]) -> bool {
    let request = latest_request(context);
    (!explanatory_request(&request) || lookup_request(&request))
        && !command_execution(context)
        && (lookup_request(&request) || phone_task(context) || computer_task(context))
}

pub(super) fn execution_tools(context: &[Value]) -> Value {
    filter(tools(), !command_execution(context))
}

pub(super) fn desktop_target(context: &[Value]) -> bool {
    !phone_task(context) && (computer_task(context) || super::desktop_focused(context))
}

pub(super) fn initial_observation(context: &[Value]) -> Option<(&'static str, Value)> {
    if !gui_execution(context) {
        return None;
    }
    let desktop = desktop_target(context);
    let name = if desktop {
        "computer_observe"
    } else {
        "phone_observe"
    };
    let calls = &context[evidence_start(context)..];
    if calls
        .iter()
        .any(|call| call["type"] == "function_call" && call["name"] == name)
    {
        return None;
    }
    if !desktop {
        return Some((name, json!({})));
    }
    let Some(list) = calls
        .iter()
        .rev()
        .find(|call| call["type"] == "function_call" && call["name"] == "device_list")
    else {
        return Some(("device_list", json!({})));
    };
    let value = tool_output(context, list).and_then(tool_value)?;
    Some((name, json!({"device_id":single_desktop(&value)?})))
}

fn single_desktop(value: &Value) -> Option<&str> {
    let devices = value["devices"].as_array()?;
    let mut desktops = devices.iter().filter(|device| device["kind"] == "desktop");
    let device = desktops.next()?;
    if desktops.next().is_some() {
        return None;
    }
    device["device_id"].as_str()
}

#[cfg(test)]
mod execution_policy_tests {
    use super::*;
    use crate::worker::finish_rejection;

    fn request(text: &str) -> Vec<Value> {
        vec![json!({"role":"user","content":[{"type":"input_text","text":text}]})]
    }

    #[test]
    fn ordinary_device_research_cannot_offer_a_command_shortcut() {
        let catalog = execution_tools(&request("找园艺相关的企业，记到电脑的表格里"));
        let names: Vec<_> = catalog
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        for name in [
            "computer_run",
            "workspace_run",
            "workspace_fetch",
            "phone_shell",
        ] {
            assert!(
                !names.contains(&name),
                "{name} bypasses the visible device task"
            );
        }
        assert!(names.contains(&"computer_observe"));
        assert!(names.contains(&"phone_observe"));
        assert!(names.contains(&"workspace_write"));
        assert!(names.contains(&"skill_install"));
    }

    #[test]
    fn commands_need_a_positive_purpose_even_without_device_or_search_words() {
        for text in [
            "阿里巴巴最近发展怎么样",
            "帮我了解一下今天的市场",
            "OpenAI latest developments",
            "在电脑浏览器搜索root教程",
            "在手机搜索shell的资料",
            "找root教程",
            "了解一下shell的用法",
            "不要用命令检查磁盘",
            "Do not use shell to check disk",
            "不要使用命令检查磁盘",
        ] {
            let catalog = execution_tools(&request(text));
            assert!(
                catalog
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|tool| { !command_tool(tool["name"].as_str().unwrap_or("")) }),
                "ordinary request exposed a command shortcut: {text}"
            );
        }
    }

    #[test]
    fn provider_native_search_is_never_part_of_the_registered_tool_contract() {
        for restricted in [false, true] {
            let catalog = filter(
                json!([
                    {"type":"web_search"},
                    {"type":"google_search"},
                    {"type":"function","name":"computer_observe"}
                ]),
                restricted,
            );
            assert_eq!(
                catalog,
                json!([{"type":"function","name":"computer_observe"}])
            );
        }
    }

    #[test]
    fn explicit_commands_and_local_programming_keep_the_harness() {
        for text in [
            "用命令检查电脑磁盘",
            "用 Python 处理电脑里的本地表格",
            "Use shell to check disk space",
            "执行ADB维护命令，打开系统设置验证",
            "不要删除原件，但用命令检查电脑磁盘",
        ] {
            assert!(
                execution_tools(&request(text))
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "computer_run")
            );
        }
    }

    #[test]
    fn device_actions_and_unscoped_research_both_require_visible_tools() {
        for text in ["打开电脑上的文档", "搜索世界新闻"] {
            assert!(gui_execution(&request(text)));
            assert!(
                !execution_tools(&request(text))
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "computer_run")
            );
        }
    }

    #[test]
    fn searching_for_coding_topics_does_not_authorize_script_research() {
        assert!(gui_execution(&request("在电脑浏览器搜索写代码的教程")));
        assert!(!gui_execution(&request("只解释搜索引擎原理，不要调用工具")));
        assert!(!browser_lookup("在微信搜索好友"));
        assert!(!lookup_request("检索知识库里的文档"));
    }

    fn completed(context: &mut Vec<Value>, name: &str, arguments: Value, text: &str) {
        let id = format!("{}-{name}", context.len());
        context.push(json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()}));
        context.push(json!({"type":"function_call_output","call_id":id,"output":[{"type":"input_text","text":text}]}));
    }

    #[test]
    fn browser_lookup_cannot_complete_from_an_observation_and_curl() {
        let mut context = request("在电脑搜索安防企业");
        completed(
            &mut context,
            "skill_read",
            json!({"name":"computer-use"}),
            "Skill id=computer-use，enabled=true",
        );
        completed(
            &mut context,
            "computer_observe",
            json!({}),
            "{\"success\":true,\"observation_id\":\"desktop\",\"title\":\"Mousepad\"}",
        );
        completed(
            &mut context,
            "computer_run",
            json!({"script":"curl https://example.test"}),
            "{\"success\":true}",
        );
        assert!(
            finish_rejection(&context, "已找到企业").is_some(),
            "terminal fetching is not browser evidence"
        );
        completed(
            &mut context,
            "computer_type",
            json!({"text":"安防企业"}),
            "{\"success\":true,\"observation_id\":\"browser\",\"title\":\"安防企业 - Chromium\"}",
        );
        assert!(finish_rejection(&context, "已找到企业").is_none());
    }

    #[test]
    fn in_app_lookup_requires_interaction_but_not_a_browser() {
        let mut context = request("在电脑的Mousepad文档中查找项目名称");
        assert!(!browser_lookup(&latest_request(&context)));
        completed(
            &mut context,
            "computer_observe",
            json!({}),
            r#"{"success":true,"observation_id":"first","title":"Mousepad"}"#,
        );
        assert!(!lookup_evidence(&context));
        completed(
            &mut context,
            "computer_key",
            json!({"key":"ctrl+f"}),
            r#"{"success":true,"observation_id":"find","title":"Mousepad"}"#,
        );
        assert!(lookup_evidence(&context));
        let mut online = request("在电脑搜索安防公司");
        completed(
            &mut online,
            "computer_key",
            json!({"key":"ctrl+f"}),
            r#"{"success":true,"observation_id":"find","title":"Mousepad"}"#,
        );
        assert!(!lookup_evidence(&online));
        completed(
            &mut online,
            "workspace_read",
            json!({}),
            r#"{"observation_id":"fake","title":"Chromium"}"#,
        );
        assert!(
            !lookup_evidence(&online),
            "workspace text cannot supply a device frame"
        );
        assert!(!browser_frame(
            &json!({"observation_id":"fake","title":"Chromium - Mousepad"})
        ));
    }

    #[test]
    fn offline_in_app_lookup_accepts_real_gui_evidence_without_a_browser() {
        for text in [
            "在当前 Writer 文档中用界面查找42.25，确认找到后关闭查找栏。不修改或关闭文档，不联网、不下载、不发送外部消息。",
            "在微信查找联系人，不联网，不使用浏览器",
            "Search the Writer document without browser or online access",
        ] {
            let mut context = request(text);
            assert!(lookup_request(text), "{text}");
            assert!(
                !browser_lookup(text),
                "negative network instruction became browser requirement: {text}"
            );
            completed(
                &mut context,
                "computer_key",
                json!({"key":"ctrl+f"}),
                r#"{"success":true,"observation_id":"find","title":"LibreOffice Writer"}"#,
            );
            assert!(lookup_evidence(&context), "{text}");
        }
        assert!(browser_lookup(
            "在电脑浏览器搜索资料，不下载、不发送外部消息"
        ));
    }

    #[test]
    fn read_only_screen_reports_start_with_observation_not_model_planning() {
        for text in [
            "开发只读验收：只观察手机当前小红书画面，报告前台应用与能直接看到的页面状态。不操作微信。保持所有应用和内容原样。",
            "观察手机屏幕，报告实际状态",
            "只观察手机当前画面，不点击也不输入",
            "看看手机当前画面并报告你看到了什么",
            "Inspect the Android screen and report its current state",
        ] {
            assert_eq!(
                initial_observation(&request(text)),
                Some(("phone_observe", json!({}))),
                "{text}"
            );
            assert!(
                !crate::worker::phone_action_required(&request(text)),
                "read-only inspection must not demand input: {text}"
            );
        }
        for text in [
            "不要观察手机，只解释原理",
            "编写一份关于手机的报告，不要调用工具",
            "在工作目录读取手机截图文件，报告文件信息",
            "Do not inspect the phone; explain only",
        ] {
            assert!(initial_observation(&request(text)).is_none(), "{text}");
        }
    }

    #[test]
    fn desktop_affinity_selects_the_device_without_turning_chat_into_gui_work() {
        let focused = |text: &str| {
            let mut context = vec![
                json!({"role":"system","content":[{"type":"input_text","text":"[当前设备类型:desktop]"}]}),
            ];
            context.extend(request(text));
            context
        };
        for text in ["你好", "只读取Skill列表，不操作设备", "把这句话记入知识库"]
        {
            assert!(initial_observation(&focused(text)).is_none(), "{text}");
        }
        for text in ["打开浏览器", "只观察当前画面并报告状态", "创建一份文档"]
        {
            assert_eq!(
                initial_observation(&focused(text)).unwrap().0,
                "device_list",
                "{text}"
            );
        }
        assert!(initial_observation(&focused("用 Python 处理电脑里的本地表格")).is_none());
        assert!(desktop_target(&focused("阿里巴巴最近发展怎么样")));
        assert!(initial_observation(&focused("阿里巴巴最近发展怎么样")).is_none());
        assert_eq!(
            initial_observation(&focused("搜索GitHub上的skill"))
                .unwrap()
                .0,
            "device_list"
        );
    }

    #[test]
    fn first_model_call_can_start_with_a_real_frame_without_a_planning_round_trip() {
        let mut context = request("在电脑搜索安防企业");
        assert_eq!(initial_observation(&context).unwrap().0, "device_list");
        completed(
            &mut context,
            "device_list",
            json!({}),
            "{\"devices\":[{\"device_id\":\"desktop-1\",\"kind\":\"desktop\"},{\"device_id\":\"phone-1\",\"kind\":\"phone\"}]}",
        );
        assert_eq!(
            initial_observation(&context),
            Some(("computer_observe", json!({"device_id":"desktop-1"})))
        );
        completed(
            &mut context,
            "computer_observe",
            json!({}),
            "{\"success\":false}",
        );
        assert!(
            initial_observation(&context).is_none(),
            "a failed observation is not blindly retried"
        );
        context.extend(request("继续"));
        assert_eq!(initial_observation(&context).unwrap().0, "device_list");
        assert!(initial_observation(&request("你好")).is_none());
    }
}
