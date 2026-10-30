use ai_phone_api::wire;
use axum::{
    Router,
    body::Bytes,
    extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use prost::Message;
use std::net::SocketAddr;

const PROTOBUF: &str = "application/x-protobuf";

fn protobuf(status: StatusCode, message: impl Message) -> Response {
    (status, [(CONTENT_TYPE, PROTOBUF)], message.encode_to_vec()).into_response()
}

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    protobuf(
        status,
        wire::ApiError {
            code: code.into(),
            message: message.into(),
        },
    )
}

async fn probe(
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    let media_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !media_type.is_some_and(|value| value.eq_ignore_ascii_case(PROTOBUF)) {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Use application/x-protobuf",
        );
    }
    let body = match body {
        Ok(body) => body,
        Err(rejection) => {
            return error(
                rejection.status(),
                "body_rejected",
                "Cannot read request body",
            );
        }
    };
    let request = match wire::ProbeRequest::decode(body) {
        Ok(request) => request,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_protobuf",
                "Cannot decode ProbeRequest",
            );
        }
    };
    if request.nonce.is_empty() || request.nonce.len() > 128 {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_nonce",
            "nonce must contain 1 to 128 UTF-8 bytes",
        );
    }
    protobuf(
        StatusCode::OK,
        wire::ProbeResponse {
            nonce: request.nonce,
            service: "ai-phone-api".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            stage: "preparation".into(),
        },
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    // Loopback is deliberate until real authentication and LAN ingress are ready.
    let addr: SocketAddr = std::env::var("AI_PHONE_API_BIND")
        .unwrap_or_else(|_| "127.0.0.1:17861".into())
        .parse()?;
    let app = Router::new()
        .route("/healthz", get(|| async { "alive" }))
        .route("/api/v1/system/probe", post(probe))
        .layer(DefaultBodyLimit::max(4096));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, stage = "preparation", "AI Phone API scaffold listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
