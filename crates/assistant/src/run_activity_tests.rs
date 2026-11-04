//! Continued user runs keep their original ID; activity, not creation, owns status.
use super::*;
use sqlx::Row;

#[tokio::test]
#[ignore = "real disposable PostgreSQL; resumed conversation status selection"]
async fn latest_conversation_status_tracks_resumed_activity_and_keeps_active_priority() {
    worker::steering_regressions::database_case(|app| async move {
        let conversation=Uuid::now_v7();
        sqlx::query("INSERT INTO conversations(id,device_id) VALUES($1,'activity-fixture')").bind(conversation).execute(&app.db).await.unwrap();
        let mut ids=vec![];
        for (origin,age,finished) in [("user",10,0),("exploration",1,60)] {
            let run=Uuid::now_v7();let message=Uuid::now_v7();ids.push(run);
            sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')").bind(message).bind(conversation).execute(&app.db).await.unwrap();
            sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,origin,created_at,input_at,finished_at) VALUES($1,$2,$3,$1,'fixture','succeeded',$4,now()-make_interval(days=>$5),now()-make_interval(secs=>$6),now()-make_interval(secs=>$6))")
                .bind(run).bind(conversation).bind(message).bind(origin).bind(age).bind(finished as f64).execute(&app.db).await.unwrap();
        }
        let mut tx=app.db.begin().await.unwrap();
        let current=http::current_run(&mut tx,conversation).await.unwrap().unwrap();
        assert_eq!(current.get::<Uuid,_>("id"),ids[0],"newly completed input in an older continuous run must not display a stale background status");
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE runs SET status='queued' WHERE id=$1").bind(ids[1]).execute(&app.db).await.unwrap();
        let mut tx=app.db.begin().await.unwrap();
        assert_eq!(http::current_run(&mut tx,conversation).await.unwrap().unwrap().get::<Uuid,_>("id"),ids[1],"active execution still takes priority over completed activity");
        tx.rollback().await.unwrap();
    }).await;
}
