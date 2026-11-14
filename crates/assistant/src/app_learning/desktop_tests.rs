use super::*;

#[tokio::test]
#[ignore = "real disposable PostgreSQL; desktop experience promotion and recall"]
async fn desktop_evidence_promotes_and_recalls_without_phone_or_title_fallback() {
    crate::worker::steering_regressions::database_case(|app|async move{
        crate::registry::initialize(&app.db,&app.settings).await.unwrap();
        crate::builtin_guides::test_files(&app.root);
        crate::workbench::initialize(&app.db,&app.root).await.unwrap();
        let device="learning-desktop";let conversation:Uuid=sqlx::query_scalar("SELECT id FROM conversations WHERE owner_id='local-user'").fetch_one(&app.db).await.unwrap();let message=Uuid::now_v7();let run=Uuid::now_v7();
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES($1,'Desktop','desktop','Linux','managed')").bind(device).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')").bind(message).bind(conversation).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,device_id) VALUES($1,$2,$3,$1,'fixture','running',$4)").bind(run).bind(conversation).bind(message).bind(device).execute(&app.db).await.unwrap();
        let mut output=Value::Null;
        for (id,after) in [("desktop-before",false),("desktop-after",true)] {
            let event=Uuid::now_v7();
            sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,status) VALUES($1,$2,$3,'tool','desktop fixture','completed')").bind(event).bind(run).bind(message).execute(&app.db).await.unwrap();
            output=json!([{"type":"input_text","text":json!({"success":true,"device_id":device,"observation_id":id,"application_id":"linux:libreoffice-writer","title":"private-doc.odt — Writer"}).to_string()}]);
            observe(&app,run,event,after,&output).await.unwrap();
        }
        let candidate=json!({"app_id":"linux:libreoffice-writer","title":"打开并关闭查找","procedure":"在当前Writer窗口按Ctrl+F，核对查找栏，再按Esc关闭。","outcome":"verified","limitations":"只验证当前Writer版本的查找栏。","observation_ids":["desktop-before","desktop-after"]});
        assert!(crate::completion_audit::retain_learning(&app,run,false,Some(&candidate)).await);
        assert!(promote(&app,run).await.is_err());
        sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,status,finished_at) VALUES($1,$2,$3,'audit','fixture approval','completed',now())").bind(Uuid::now_v7()).bind(run).bind(message).execute(&app.db).await.unwrap();
        promote(&app,run).await.unwrap();
        let name=recall(&app,"linux:libreoffice-writer").await.unwrap().unwrap();
        let body=crate::workbench::skill_text(&app,&name).await.unwrap();assert!(!body.contains("private-doc.odt"));
        let mut context=vec![json!({"role":"user","content":[{"type":"input_text","text":"在电脑Writer查找"}]}),json!({"type":"function_call","name":"computer_key","call_id":"latest","arguments":"{}"}),json!({"type":"function_call_output","call_id":"latest","output":output})];
        let mut state=crate::agent_loop::LoopState::default();
        let guide=crate::worker::experience_call(&app,&context,&mut state).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(guide["arguments"].as_str().unwrap()).unwrap()["name"],"writer");
        context.push(guide.clone());
        context.push(json!({"type":"function_call_output","call_id":guide["call_id"],"output":[{"type":"input_text","text":crate::workbench::skill_text(&app,"writer").await.unwrap()}]}));
        let call=crate::worker::experience_call(&app,&context,&mut state).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap()["name"],name);
        assert!(crate::worker::experience_call(&app,&context,&mut state).await.is_none());
        context.push(json!({"type":"function_call","name":"computer_observe","call_id":"unknown-app","arguments":"{}"}));
        context.push(json!({"type":"function_call_output","call_id":"unknown-app","output":[{"type":"input_text","text":json!({"success":true,"device_id":device,"observation_id":"unknown","application_id":"","title":"private-doc.odt — Writer"}).to_string()}]}));
        assert!(crate::worker::experience_call(&app,&context,&mut crate::agent_loop::LoopState::default()).await.is_none(),"unknown latest application cannot borrow older identity");
        let mut wrong=candidate.clone();wrong["app_id"]=json!("com.example.writer");assert!(save(&app,run,&wrong).await.is_err());
        assert!(recall(&app,"libreoffice-writer").await.unwrap().is_none());
        sqlx::query("UPDATE runtime_skills SET enabled=false,content=content||E'\\nManual note remains' WHERE name=$1").bind(&name).execute(&app.db).await.unwrap();
        promote(&app,run).await.unwrap();assert!(recall(&app,"linux:libreoffice-writer").await.unwrap().is_none());
        assert!(crate::workbench::skill_text(&app,&name).await.unwrap().contains("Manual note remains"));
    }).await;
}
