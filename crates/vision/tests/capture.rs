#![cfg(unix)]
use ai_phone_vision::{Error, recognize};
use std::{os::unix::fs::PermissionsExt, path::Path};

fn frame() -> serde_json::Value {
    serde_json::json!({"width":720,"height":1440,"grid":vec![100;2048],"regions":[
        {"text":"设置","confidence":0.95,"bounds":{"left":100,"top":200,"right":200,"bottom":250}}
    ]})
}

#[tokio::test]
async fn finite_input_closes_stdin_and_preserves_full_frame_coordinates() {
    let bytes = serde_json::to_vec(&frame()).unwrap();
    let capture = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        recognize(Path::new("/bin/cat"), &bytes, 720, 1440),
    )
    .await
    .expect("child must receive EOF")
    .unwrap();
    assert_eq!(capture.grid, vec![100; 2048]);
    let region = &capture.regions[0];
    assert_eq!(region.text, "设置");
    assert_eq!((region.bounds.left, region.bounds.top), (100, 200));
}

#[tokio::test]
async fn mismatched_frames_and_unbounded_output_are_rejected() {
    let mut value = frame();
    value["width"] = 721.into();
    assert!(matches!(
        recognize(
            Path::new("/bin/cat"),
            &serde_json::to_vec(&value).unwrap(),
            720,
            1440
        )
        .await,
        Err(Error::Dimensions)
    ));
    value["width"] = 720.into();
    value["grid"] = serde_json::json!([0]);
    assert!(matches!(
        recognize(
            Path::new("/bin/cat"),
            &serde_json::to_vec(&value).unwrap(),
            720,
            1440
        )
        .await,
        Err(Error::Dimensions)
    ));
    assert!(matches!(
        recognize(Path::new("/bin/cat"), b"not json", 720, 1440).await,
        Err(Error::InvalidData)
    ));
    let oversized = vec![b' '; 262145];
    assert!(matches!(
        recognize(Path::new("/bin/cat"), &oversized, 720, 1440).await,
        Err(Error::Failed)
    ));
}

#[tokio::test]
async fn launch_errors_do_not_execute_non_executable_files() {
    let path = std::env::temp_dir().join(format!("aiphone-ocr-noexec-{}", std::process::id()));
    std::fs::write(&path, b"non-executable fixture").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let result = recognize(&path, b"fixture", 720, 1440).await;
    std::fs::remove_file(path).unwrap();
    assert!(
        matches!(result, Err(Error::Launch(ref error)) if error.kind()==std::io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn text_search_keeps_ambiguous_positions_and_filters_invalid_boxes() {
    let mut value = frame();
    value["regions"] = serde_json::json!([
        {"text":"OK","confidence":0.95,"bounds":{"left":620,"top":630,"right":665,"bottom":655}},
        {"text":"OK","confidence":0.92,"bounds":{"left":300,"top":200,"right":345,"bottom":225}},
        {"text":"Bookmarks","confidence":0.99,"bounds":{"left":300,"top":30,"right":420,"bottom":55}},
        {"text":"OK","confidence":0.40,"bounds":{"left":10,"top":200,"right":50,"bottom":225}},
        {"text":"OK","confidence":0.95,"bounds":{"left":4294967295u32,"top":200,"right":20,"bottom":225}}
    ]);
    let capture: ai_phone_vision::Capture = serde_json::from_value(value).unwrap();
    let found = capture.find_text(" ok ");
    assert_eq!(found.len(), 2, "ambiguous labels must remain separate");
    assert_eq!((found[0].bounds.left, found[0].bounds.top), (620, 630));
    assert_eq!(found[1].bounds.left, 300);
    assert_eq!(capture.find_text("book")[0].text, "Bookmarks");
    assert!(capture.find_text("missing").is_empty());
    assert!(capture.find_text("   ").is_empty());
}

#[test]
fn visible_edge_labels_and_confidence_threshold_remain_searchable() {
    let region = |confidence, bounds| serde_json::json!({"text":"边界","confidence":confidence,"bounds":bounds});
    let edge = serde_json::json!({"left":716,"top":1436,"right":720,"bottom":1440});
    let mut value = frame();
    value["regions"] = serde_json::json!([
        region(0.80, edge.clone()),
        region(0.799, edge),
        region(
            0.95,
            serde_json::json!({"left":717,"top":1436,"right":720,"bottom":1440})
        ),
        region(
            0.95,
            serde_json::json!({"left":716,"top":1437,"right":720,"bottom":1440})
        ),
        region(
            0.95,
            serde_json::json!({"left":716,"top":1436,"right":721,"bottom":1440})
        ),
        region(
            0.95,
            serde_json::json!({"left":716,"top":1436,"right":720,"bottom":1441})
        )
    ]);
    let capture: ai_phone_vision::Capture = serde_json::from_value(value).unwrap();
    let found = capture.find_text("边界");
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].bounds.right, found[0].bounds.bottom), (720, 1440));
    assert_eq!(found[0].confidence, 0.80);
}

#[test]
fn repeated_labels_have_a_bounded_result_list() {
    let mut value = frame();
    let region = value["regions"][0].clone();
    value["regions"] = serde_json::Value::Array(vec![region; 21]);
    let capture: ai_phone_vision::Capture = serde_json::from_value(value).unwrap();
    assert_eq!(capture.find_text("设置").len(), 20);
}
