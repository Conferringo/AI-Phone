use crate::*;
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

pub async fn initialize(db: &PgPool, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM admin_account)")
        .fetch_one(db)
        .await?;
    let path = root.join("data/assistant/setup-code");
    if exists {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    fs::create_dir_all(path.parent().ok_or("Missing setup directory")?)?;
    use std::os::unix::fs::OpenOptionsExt;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(
                format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()).as_bytes(),
            )?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    println!(
        "First owner setup is available at /setup. Read the local data/assistant/setup-code file to claim it."
    );
    Ok(())
}
pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(operation): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let result:Result<Response,String>=async {
        if operation=="status" {
            if let Err(e)=decode::<wire::SettingsRequest>(&app,&headers,body){return Ok(e.into());}
            let owner_created:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM admin_account)").fetch_one(&app.db).await.map_err(|_|"无法读取初始化状态")?;
            return Ok(pb(StatusCode::OK,wire::SetupState{owner_created,model_configured:crate::registry::configured(&app).await}));
        }
        if operation!="owner" {return Ok(error(StatusCode::NOT_FOUND,"not_found","操作不存在"));}
        let request=match decode::<wire::SetupOwnerRequest>(&app,&headers,body){Ok(v)=>v,Err(e)=>return Ok(e.into())};
        {let mut attempts=app.login_attempts.lock().await;attempts.retain(|t|t.elapsed()<Duration::from_secs(60));if attempts.len()>=20{return Ok(error(StatusCode::TOO_MANY_REQUESTS,"rate_limit","请稍后再试"));}attempts.push(std::time::Instant::now());}
        if !(8..=128).contains(&request.password.len()) {return Err("新密码需为 8–128 个字节".into());}
        let path=app.root.join("data/assistant/setup-code");
        let saved=fs::read_to_string(&path).map_err(|_|"初始化已关闭或本机初始化码不可用")?;
        let a=Sha256::digest(saved.trim().as_bytes());let b=Sha256::digest(request.setup_code.trim().as_bytes());
        if a.iter().zip(b).fold(0u8,|n,(a,b)|n|(a^b))!=0{return Err("初始化码不正确".into());}
        let hash=tokio::task::spawn_blocking(move||{
            let salt=SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_|"无法生成密码盐")?;
            Argon2::default().hash_password(request.password.as_bytes(),&salt).map(|h|h.to_string()).map_err(|_|"无法保存密码")
        }).await.map_err(|_|"无法保存密码")??;
        // The database singleton is the authority. A racing setup can never reset an owner.
        let result=sqlx::query("INSERT INTO admin_account(singleton,username,password_hash) VALUES(true,'admin',$1) ON CONFLICT(singleton) DO NOTHING").bind(hash).execute(&app.db).await.map_err(|_|"无法创建管理员")?;
        if result.rows_affected()!=1{return Err("管理员已存在，请登录".into());}
        let _=fs::remove_file(path);
        Ok(pb(StatusCode::CREATED,wire::SetupState{owner_created:true,model_configured:crate::registry::configured(&app).await}))
    }.await;
    result.unwrap_or_else(|message| error(StatusCode::CONFLICT, "setup_error", &message))
}
