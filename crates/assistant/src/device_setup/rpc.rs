use super::{Error, Result};
use prost::Message;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MEDIA: &str = "application/x-protobuf";
const MAX_RESPONSE: usize = 32 * 1024 * 1024;

pub(super) struct Rpc {
    client: reqwest::Client,
    assistant_token: String,
    runner_token: String,
    pub cancelled: CancellationToken,
    assistant_port: u16,
    runner_port: u16,
}
impl Rpc {
    pub fn new(
        assistant_token: String,
        runner_token: String,
        cancelled: CancellationToken,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| Error("device_setup_transport"))?;
        Ok(Self {
            client,
            assistant_token,
            runner_token,
            cancelled,
            assistant_port: 17863,
            runner_port: 17862,
        })
    }
    #[cfg(test)]
    pub fn fixture_ports(mut self, assistant: u16, runner: u16) -> Self {
        self.assistant_port = assistant;
        self.runner_port = runner;
        self
    }
    pub async fn call<I: Message, O: Message + Default>(&self, route: &str, input: I) -> Result<O> {
        let bytes = self
            .exchange(
                self.assistant_port,
                route,
                &self.assistant_token,
                input.encode_to_vec(),
                Duration::from_secs(120),
            )
            .await?;
        O::decode(bytes.as_slice()).map_err(|_| Error("device_setup_protobuf"))
    }
    pub async fn phone(
        &self,
        input: crate::wire::DeviceRequest,
    ) -> Result<crate::wire::DeviceObservation> {
        let bytes = self
            .exchange(
                self.runner_port,
                "observe",
                &self.runner_token,
                input.encode_to_vec(),
                Duration::from_secs(30),
            )
            .await?;
        crate::wire::DeviceObservation::decode(bytes.as_slice())
            .map_err(|_| Error("device_setup_protobuf"))
    }
    pub async fn notifications(
        &self,
        input: crate::wire::DeviceRequest,
    ) -> Result<crate::wire::PhoneNotificationList> {
        let bytes = self
            .exchange(
                self.runner_port,
                "notifications",
                &self.runner_token,
                input.encode_to_vec(),
                Duration::from_secs(5),
            )
            .await?;
        crate::wire::PhoneNotificationList::decode(bytes.as_slice())
            .map_err(|_| Error("device_setup_protobuf"))
    }
    async fn exchange(
        &self,
        port: u16,
        route: &str,
        token: &str,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => Err(Error("device_setup_cancelled_remote_may_continue")),
            result = self.request(port, route, token, body, timeout) => result,
        }
    }
    async fn request(
        &self,
        port: u16,
        route: &str,
        token: &str,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let mut response = self
            .client
            .post(format!("http://127.0.0.1:{port}/v1/{route}"))
            .timeout(Duration::from_secs(if port == self.runner_port {
                30
            } else {
                120
            }))
            .bearer_auth(token)
            .header("content-type", MEDIA)
            .body(body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|_| Error("device_setup_response_unknown"))?;
        if response.status() != reqwest::StatusCode::OK
            || response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                != Some(MEDIA)
        {
            return Err(Error("device_setup_response_rejected"));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE as u64)
        {
            return Err(Error("device_setup_response_size"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error("device_setup_response_unknown"))?
        {
            if chunk.len() > MAX_RESPONSE - bytes.len() {
                return Err(Error("device_setup_response_size"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}
