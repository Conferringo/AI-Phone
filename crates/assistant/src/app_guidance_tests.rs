//! The real persistence boundary decides which installed guide the worker reads.
use super::*;
use serde_json::{Value, json};

fn observed() -> Vec<Value> {
    vec![
        json!({"role":"user","content":[{"type":"input_text","text":"在手机小红书搜索"}]}),
        json!({"type":"function_call","name":"phone_observe","call_id":"observation","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"observation","output":[{"type":"input_text","text":json!({"success":true,"device_id":"phone-local-01","observation_id":"current","foreground_app":"com.xingin.xhs/.Dashboard"}).to_string()}]}),
    ]
}
fn loaded(context: &mut Vec<Value>, call: Value, text: String) {
    let output = json!({"type":"function_call_output","call_id":call["call_id"],"output":[{"type":"input_text","text":text}]});
    context.push(call);
    context.push(output);
}
fn name(call: &Value) -> String {
    serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap()["name"]
        .as_str()
        .unwrap()
        .into()
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; installed app guides precede local experience"]
async fn built_in_app_guidance_and_personal_experience_load_once_without_extra_model_calls() {
    worker::steering_regressions::database_case(|app|async move{
        crate::builtin_guides::test_files(&app.root);
        crate::workbench::initialize(&app.db,&app.root).await.unwrap();
        // A fresh install has useful shipped guidance without any private experience.
        let mut fresh=observed();let mut fresh_state=agent_loop::LoopState::default();
        let shipped=worker::experience_call(&app,&fresh,&mut fresh_state).await.unwrap();
        assert_eq!(name(&shipped),"xiaohongshu");
        loaded(&mut fresh,shipped,crate::workbench::skill_text(&app,"xiaohongshu").await.unwrap());
        assert!(worker::experience_call(&app,&fresh,&mut fresh_state).await.is_none());
        let experience="app-experience-10e91357b2f2";
        sqlx::query("INSERT INTO runtime_skills(id,name,source,content) VALUES($1,$1,'app-learning','Personal verified method')").bind(experience).execute(&app.db).await.unwrap();
        let mut context=observed();let mut state=agent_loop::LoopState::default();
        let guide=worker::experience_call(&app,&context,&mut state).await.unwrap();
        assert_eq!(name(&guide),"xiaohongshu");
        loaded(&mut context,guide.clone(),crate::workbench::skill_text(&app,"xiaohongshu").await.unwrap());
        let method=worker::experience_call(&app,&context,&mut state).await.unwrap();
        assert_eq!(name(&method),experience);
        loaded(&mut context,method,crate::workbench::skill_text(&app,experience).await.unwrap());
        assert!(worker::experience_call(&app,&context,&mut state).await.is_none());
        let mut resumed:agent_loop::LoopState=serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert!(worker::experience_call(&app,&context,&mut resumed).await.is_none());

        let mut unavailable=observed();
        loaded(&mut unavailable,guide,"{\"success\":false,\"error\":\"fixture unavailable\"}".into());
        let method=worker::experience_call(&app,&unavailable,&mut agent_loop::LoopState::default()).await.unwrap();
        assert_eq!(name(&method),experience,"one failed optional guide must not repeat or hide local experience");
        sqlx::query("UPDATE runtime_skills SET enabled=false WHERE name=$1").bind(experience).execute(&app.db).await.unwrap();
        assert!(worker::experience_call(&app,&context,&mut agent_loop::LoopState::default()).await.is_none());
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT content FROM runtime_skills WHERE name=$1").bind(experience).fetch_one(&app.db).await.unwrap(),"Personal verified method");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; fresh Office guides and checkpoint reuse"]
async fn office_guides_install_and_follow_the_observed_application_once() {
    worker::steering_regressions::database_case(|app| async move {
        crate::builtin_guides::test_files(&app.root);
        crate::workbench::initialize(&app.db, &app.root).await.unwrap();
        for guide in ["calc", "impress", "draw"] {
            let text = crate::workbench::skill_text(&app, guide).await.unwrap();
            assert!(text.contains(&format!("name: {guide}\n")));
            let application = format!("linux:libreoffice-{guide}");
            let mut context = vec![
                json!({"role":"user","content":[{"type":"input_text","text":"使用当前电脑应用"}]}),
                json!({"type":"function_call","name":"computer_observe","call_id":"current","arguments":"{}"}),
                json!({"type":"function_call_output","call_id":"current","output":[{"type":"input_text","text":json!({"success":true,"device_id":"desktop-fixture","observation_id":"current","application_id":application,"title":"private filename is not a binding"}).to_string()}]}),
            ];
            let mut state = agent_loop::LoopState::default();
            let call = worker::experience_call(&app, &context, &mut state).await.unwrap();
            assert_eq!(name(&call), guide);
            loaded(&mut context, call, text);
            let mut resumed = serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
            assert!(worker::experience_call(&app, &context, &mut resumed).await.is_none());
        }
        // Exercise the supported user path; built-in Skills are immutable.
        let cancel = tokio_util::sync::CancellationToken::new();
        let original = "---\nname: office-notes\ndescription: Personal Office notes.\n---\n\nOriginal notes\n";
        let item = crate::workbench::invoke(&app, "skill_create", &json!({"content":original}), &cancel).await.unwrap();
        let updated = original.replace("Original notes", "Manual Office guide");
        crate::workbench::invoke(&app, "skill_edit", &json!({"id":item["id"],"content":updated,"expected_content_version":item["content_version"]}), &cancel).await.unwrap();
        crate::workbench::invoke(&app, "skill_set_enabled", &json!({"id":item["id"],"enabled":false}), &cancel).await.unwrap();
        crate::workbench::initialize(&app.db, &app.root).await.unwrap();
        let (body, enabled): (String, bool) = sqlx::query_as("SELECT content,enabled FROM runtime_skills WHERE id=$1")
            .bind(item["id"].as_str().unwrap()).fetch_one(&app.db).await.unwrap();
        assert_eq!(body, updated);
        assert!(!enabled);
    }).await;
}
