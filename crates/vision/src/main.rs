use std::{
    io::{Read, Write},
    process::ExitCode,
};
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("ai-phone-vision {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let mut png = vec![];
    let result = async {
        std::io::stdin()
            .take(2_097_153)
            .read_to_end(&mut png)
            .map_err(|_| ai_phone_vision::Error::InvalidData)?;
        let capture = ai_phone_vision::tesseract::recognize(&png).await?;
        let bytes =
            serde_json::to_vec(&capture).map_err(|_| ai_phone_vision::Error::InvalidData)?;
        std::io::stdout()
            .write_all(&bytes)
            .map_err(ai_phone_vision::Error::Read)
    }
    .await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Phone OCR unavailable: {error}");
            ExitCode::FAILURE
        }
    }
}
