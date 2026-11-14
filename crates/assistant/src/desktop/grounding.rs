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

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Viewport {
    x: u32,
    y: u32,
    width: std::num::NonZeroU32,
    height: std::num::NonZeroU32,
}
impl Viewport {
    fn fits(&self, observation: &wire::DesktopObservation) -> bool {
        self.x
            .checked_add(self.width.get())
            .is_some_and(|right| right <= observation.width)
            && self
                .y
                .checked_add(self.height.get())
                .is_some_and(|bottom| bottom <= observation.height)
    }
}
#[derive(serde::Deserialize)]
struct Zoomed {
    png: String,
    width: std::num::NonZeroU32,
    height: std::num::NonZeroU32,
}
fn viewport(args: &Value, observation: &wire::DesktopObservation) -> Result<Viewport> {
    let viewport: Viewport =
        if let Some(value) = args.get("region").filter(|value| !value.is_null()) {
            serde_json::from_value(value.clone())
                .map_err(|_| "定位区域需要x、y、width、height".to_string())?
        } else {
            Viewport {
                x: 0,
                y: 0,
                width: std::num::NonZeroU32::new(observation.width).ok_or("无效画面宽度")?,
                height: std::num::NonZeroU32::new(observation.height).ok_or("无效画面高度")?,
            }
        };
    if !viewport.fits(observation) {
        return Err("定位区域必须在当前画面内，宽高为正整数像素".into());
    }
    Ok(viewport)
}
async fn frame(
    observation: &wire::DesktopObservation,
    viewport: &Viewport,
) -> Result<(Vec<u8>, std::num::NonZeroU32, std::num::NonZeroU32)> {
    if viewport.x == 0
        && viewport.y == 0
        && viewport.width.get() == observation.width
        && viewport.height.get() == observation.height
    {
        return Ok((observation.png.clone(), viewport.width, viewport.height));
    }
    let value=helper("zoom",json!({"png":STANDARD.encode(&observation.png),"x":viewport.x,"y":viewport.y,"width":viewport.width,"height":viewport.height})).await?;
    let zoomed: Zoomed =
        serde_json::from_value(value).map_err(|_| "局部画面返回无效".to_string())?;
    let png = STANDARD
        .decode(zoomed.png)
        .map_err(|_| "局部画面编码无效".to_string())?;
    Ok((png, zoomed.width, zoomed.height))
}
fn project(
    bounds: &ai_phone_vision::Bounds,
    viewport: &Viewport,
    width: std::num::NonZeroU32,
    height: std::num::NonZeroU32,
) -> ai_phone_vision::Bounds {
    let horizontal = |edge: u32| u64::from(edge) * u64::from(viewport.width.get());
    let vertical = |edge: u32| u64::from(edge) * u64::from(viewport.height.get());
    ai_phone_vision::Bounds {
        left: viewport.x + (horizontal(bounds.left) / u64::from(width.get())) as u32,
        top: viewport.y + (vertical(bounds.top) / u64::from(height.get())) as u32,
        right: viewport.x + horizontal(bounds.right).div_ceil(u64::from(width.get())) as u32,
        bottom: viewport.y + vertical(bounds.bottom).div_ceil(u64::from(height.get())) as u32,
    }
}
async fn find_text(
    app: &App,
    observation: &wire::DesktopObservation,
    args: &Value,
) -> Result<Value> {
    let text = query_text(args)?;
    let viewport = viewport(args, observation)?;
    let (png, width, height) = frame(observation, &viewport).await?;
    let capture = ai_phone_vision::recognize(
        &ai_phone_vision::program(&app.root),
        &png,
        width.get(),
        height.get(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let candidates: Vec<_> = capture
        .find_text(text)
        .iter()
        .map(|r| {
            let b = project(&r.bounds, &viewport, width, height);
            json!({"text":r.text,"confidence":r.confidence,"bounds":b,
            "center":[b.left+(b.right-b.left)/2,b.top+(b.bottom-b.top)/2]})
        })
        .collect();
    Ok(plain(
        json!({"success":true,"device_id":observation.device_id,
        "source_observation_id":observation.observation_id,"source":"ocr","search_region":viewport,
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
    if name == "computer_find_control" {
        let value=helper("controls", json!({"window_id":observation.window_id,"process_id":observation.process_id,"text":args["text"].as_str().unwrap_or(""),"role":args["role"].as_str().unwrap_or("")})).await?;
        return Ok(plain(
            json!({"success":true,"device_id":device,"source_observation_id":observation.observation_id,
            "source":"accessibility","coordinate_space":"full_screen","candidates":value["candidates"],"partial":value["partial"],"inspected_nodes":value["inspected_nodes"],
            "note":"结合当前画面选择角色/状态正确的候选；同名控件不自动择一。"}),
        ));
    }
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
    fn crop_projection_preserves_full_screen_coordinates_and_rounded_edges() {
        let viewport: Viewport =
            serde_json::from_value(json!({"x":100,"y":200,"width":333,"height":500})).unwrap();
        let width = std::num::NonZeroU32::new(533).unwrap();
        let height = std::num::NonZeroU32::new(800).unwrap();
        let b = project(
            &ai_phone_vision::Bounds {
                left: 0,
                top: 0,
                right: 533,
                bottom: 800,
            },
            &viewport,
            width,
            height,
        );
        assert_eq!((b.left, b.top, b.right, b.bottom), (100, 200, 433, 700));
        let b = project(
            &ai_phone_vision::Bounds {
                left: 1,
                top: 1,
                right: 2,
                bottom: 2,
            },
            &viewport,
            width,
            height,
        );
        assert_eq!((b.left, b.top, b.right, b.bottom), (100, 200, 102, 202));
    }
    #[test]
    fn optional_search_region_cannot_exceed_the_source_frame() {
        let observation = wire::DesktopObservation {
            width: 1280,
            height: 800,
            ..Default::default()
        };
        let full = viewport(&json!({}), &observation).unwrap();
        assert_eq!(
            (full.x, full.y, full.width.get(), full.height.get()),
            (0, 0, 1280, 800)
        );
        for region in [
            json!({"x":378,"y":266,"width":60,"height":18}),
            json!({"x":1279,"y":799,"width":1,"height":1}),
        ] {
            assert!(viewport(&json!({"region":region}), &observation).is_ok());
        }
        for region in [
            json!({"x":0}),
            json!({"x":0,"y":0,"width":0,"height":40}),
            json!({"x":0,"y":0,"width":1281,"height":40}),
            json!({"x":1270,"y":0,"width":32,"height":40}),
            json!({"x":0,"y":u32::MAX,"width":32,"height":40}),
        ] {
            assert!(viewport(&json!({"region":region}), &observation).is_err());
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
