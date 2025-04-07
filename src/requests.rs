use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;
use ya_core_model::activity::RpcMessageError;

static CLIENT_API_URL: OnceLock<String> = OnceLock::new();

pub fn init_client_api_url() -> Result<&'static String, anyhow::Error> {
    let client_api_url = env::var("CRUNCHER_CLIENT_API_URL")
        .map_err(|e|anyhow!("CRUNCHER_CLIENT_API_URL not set: {e}. Without this variable runtime cannot connect to client API"))?;
    CLIENT_API_URL
        .set(client_api_url)
        .expect("CLIENT_API_URL can be set only once");
    let client_api_url = get_client_api_url();
    log::info!("Client API URL set to {}", *client_api_url);
    Ok(client_api_url)
}

pub fn get_client_api_url() -> &'static String {
    CLIENT_API_URL
        .get()
        .expect("CLIENT_API_URL not initialized")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkTarget {
    Factory(String),
    PublicKeyBase(String),
    Default,
}

// Async function to post WorkTarget
pub async fn send_work_target(target: WorkTarget) -> Result<(), RpcMessageError> {
    let client = reqwest::Client::new();
    let api_base = get_client_api_url();

    let target_url = format!("{api_base}/api/runners/target/set");
    let res = client
        .post(&target_url) // Replace with your actual endpoint
        .json(&target)
        .send()
        .await
        .map_err(|e| {
            log::error!("Failed to send request: {}", e);
            RpcMessageError::Activity(format!("Failed to send request {e}"))
        })?;

    if res.status().is_success() {
        log::info!("Successfully set WorkTarget {:?}", target);
        Ok(())
    } else {
        let status = res.status();
        log::error!("Failed to set WorkTarget: {} - url: {}", status, target_url);
        let text = if let Ok(text) = res.text().await {
            log::error!("Response: {}", text);
            text
        } else {
            "".to_string()
        };
        Err(RpcMessageError::Activity(format!(
            "Failed to set WorkTarget: {} {}",
            status, text
        )))
    }
}

// Async function to post WorkTarget
pub async fn start_work() -> Result<(), RpcMessageError> {
    let client = reqwest::Client::new();
    let api_base = get_client_api_url();

    let target_url = format!("{api_base}/api/runners/start");
    let res = client
        .post(&target_url) // Replace with your actual endpoint
        .send()
        .await
        .map_err(|e| {
            log::error!("Failed to send request: {}", e);
            RpcMessageError::Activity(format!("Failed to send request {e}"))
        })?;

    if res.status().is_success() {
        let message = res.text().await.unwrap_or("".to_string());
        log::info!("Successfully started runners with message: {}", message);
        Ok(())
    } else {
        let status = res.status();
        log::error!("Failed to start runners: {} - url: {}", status, target_url);
        let text = if let Ok(text) = res.text().await {
            log::error!("Response: {}", text);
            text
        } else {
            "".to_string()
        };
        Err(RpcMessageError::Activity(format!(
            "Failed to start runners: {} {}",
            status, text
        )))
    }
}

// Async function to post WorkTarget
pub async fn stop_work() -> Result<(), RpcMessageError> {
    let client = reqwest::Client::new();
    let api_base = get_client_api_url();

    let target_url = format!("{api_base}/api/runners/stop");
    let res = client
        .post(&target_url) // Replace with your actual endpoint
        .send()
        .await
        .map_err(|e| {
            log::error!("Failed to send request: {}", e);
            RpcMessageError::Activity(format!("Failed to send request {e}"))
        })?;

    if res.status().is_success() {
        let message = res.text().await.unwrap_or("".to_string());
        log::info!("Successfully stopped runners with message: {}", message);
        Ok(())
    } else {
        let status = res.status();
        log::error!("Failed to stop runners: {} - url: {}", status, target_url);
        let text = if let Ok(text) = res.text().await {
            log::error!("Response: {}", text);
            text
        } else {
            "".to_string()
        };
        Err(RpcMessageError::Activity(format!(
            "Failed to stop runners: {} {}",
            status, text
        )))
    }
}
