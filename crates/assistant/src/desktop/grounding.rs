//! Read-only inspection of an existing desktop observation. It grants no action lease.
use super::*;

fn validate(
    observation: &wire::DesktopObservation,
    device: &str,
    epoch: u64,
    latest: Option<&str>,
    time_ms: u64,
) -> Result<()> {
    if observation.device_id != device {
        return Err("观察不属于当前电脑".into());
    }
    if observation.epoch != epoch || time_ms.saturating_sub(observation.captured_at_ms) > 60_000 {
        return Err("观察已过期，请重新computer_observe".into());
    }
    if latest != Some(observation.observation_id.as_str()) {
        return Err("请使用最新观察，不复用旧图".into());
    }
    Ok(())
}

fn query_text(args: &Value) -> Result<&str> {
    let text = args["text"].as_str().unwrap_or("").trim();
    if text.is_empty() || text.chars().count() > 200 {
        return Err("请输入要定位的文字（1–200字）".into());
    }
    Ok(text)
}

async fn find_text(
    app: &App,
    observation: &wire::DesktopObservation,
    args: &Value,
) -> Result<Value> {
    let text = query_text(args)?;
    let bundled = app.root.join("runtime/services/bin/vision");
    let vision = std::env::var_os("AI_PHONE_OCR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if bundled.is_file() {
                bundled
            } else {
                app.root.join("target/phone-vision/recognize")
            }
        });
    let capture = ai_phone_vision::recognize(
        &vision,
        &observation.png,
        observation.width,
        observation.height,
    )
    .await
    .map_err(|error| error.to_string())?;
    let candidates: Vec<_> = capture
        .find_text(text)
        .iter()
        .map(|r| {
            let b = &r.bounds;
            json!({"text":r.text,"confidence":r.confidence,
            "bounds":{"left":b.left,"top":b.top,"right":b.right,"bottom":b.bottom},
            "center":[b.left+(b.right-b.left)/2,b.top+(b.bottom-b.top)/2]})
        })
        .collect();
    Ok(plain(
        json!({"success":true,"device_id":observation.device_id,
        "source_observation_id":observation.observation_id,"source":"ocr",
        "coordinate_space":"full_screen","candidates":candidates,
        "note":"文字位置不是可点击控件；结合当前截图核对目标、遮挡和同名结果。"}),
    ))
}

pub(super) async fn tool(
    app: &App,
    name: &str,
    device: &str,
    observation: &wire::DesktopObservation,
    args: &Value,
) -> Result<Value> {
    let row = sqlx::query("SELECT epoch,(SELECT id FROM desktop_observations WHERE device_id=$1 ORDER BY captured_at DESC,id DESC LIMIT 1) latest FROM desktop_controls WHERE device_id=$1")
        .bind(device).fetch_one(&app.db).await.map_err(|error| error.to_string())?;
    let latest = row
        .get::<Option<Uuid>, _>("latest")
        .map(|id| id.to_string());
    validate(
        observation,
        device,
        row.get::<i64, _>("epoch") as u64,
        latest.as_deref(),
        now(),
    )?;
    if name == "computer_find_text" {
        return find_text(app, observation, args).await;
    }
    let value=helper("zoom",json!({"png":STANDARD.encode(&observation.png),"x":args["x"],"y":args["y"],"width":args["width"],"height":args["height"]})).await?;
    Ok(
        json!([{"type":"input_text","text":json!({"success":true,"device_id":device,"source_observation_id":observation.observation_id,"crop_origin":[args["x"],args["y"]],"scale":value["scale"],"width":value["width"],"height":value["height"],"coordinate_rule":"操作仍使用全屏坐标：x=局部x/scale+crop_origin[0]，y同理；放大不产生新观察授权。"}).to_string()},{"type":"input_image","image_url":format!("data:image/png;base64,{}",value["png"].as_str().ok_or("放大失败")?)}]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_reuses_current_authority_without_extending_its_lifetime() {
        let observation = wire::DesktopObservation {
            device_id: "desktop".into(),
            observation_id: "frame".into(),
            epoch: 7,
            captured_at_ms: 1000,
            ..Default::default()
        };
        assert!(validate(&observation, "desktop", 7, Some("frame"), 61_000).is_ok());
        for (device, epoch, latest, time) in [
            ("phone", 7, Some("frame"), 1000),
            ("desktop", 8, Some("frame"), 1000),
            ("desktop", 7, Some("older"), 1000),
            ("desktop", 7, None, 1000),
            ("desktop", 7, Some("frame"), 61_001),
        ] {
            assert!(validate(&observation, device, epoch, latest, time).is_err());
        }
    }

    #[test]
    fn query_requires_bounded_nonempty_text() {
        assert_eq!(query_text(&json!({"text":" 设置 "})).unwrap(), "设置");
        for value in [
            json!({}),
            json!({"text":1}),
            json!({"text":"   "}),
            json!({"text":"字".repeat(201)}),
        ] {
            assert!(query_text(&value).is_err());
        }
        assert!(query_text(&json!({"text":"字".repeat(200)})).is_ok());
    }
}
