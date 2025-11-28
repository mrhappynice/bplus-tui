// ================================================
// FILE: src/api.rs
// ================================================
use anyhow::Result;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use eventsource_stream::Eventsource; 
use std::time::Duration;
use crate::app::AppAction;
use tokio::sync::mpsc::UnboundedSender;

// --- Launcher Models (UNCHANGED) ---
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppModel {
    #[serde(default, skip_serializing_if = "String::is_empty")] 
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResponse {
    pub success: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

// --- Search Models (NEW) ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSource {
    pub title: String,
    pub url: String,
    pub content: String,
    pub engine: String,
}

// --- Launcher API Functions (UNCHANGED) ---
const BASE_URL: &str = "http://localhost:5660/api/apps";
const SEARCH_URL: &str = "http://localhost:3001/api";

pub async fn fetch_apps() -> Result<Vec<AppModel>> {
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;
    let resp = client.get(BASE_URL).send().await?;
    Ok(resp.json::<Vec<AppModel>>().await?)
}

pub async fn create_app(app: &AppModel) -> Result<AppModel> {
    let client = Client::new();
    let resp = client.post(BASE_URL).json(app).send().await?;
    Ok(resp.json::<AppModel>().await?)
}

pub async fn update_app(app: &AppModel) -> Result<()> {
    let client = Client::new();
    client.put(format!("{}/{}", BASE_URL, app.id)).json(app).send().await?;
    Ok(())
}

pub async fn delete_app(id: &str) -> Result<()> {
    let client = Client::new();
    client.delete(format!("{}/{}", BASE_URL, id)).send().await?;
    Ok(())
}

pub async fn launch_app(id: String) -> Result<LaunchResponse> {
    let client = Client::new();
    let resp = client.post(format!("{}/{}/launch", BASE_URL, id)).send().await?;
    Ok(resp.json::<LaunchResponse>().await?)
}

// --- Searchrs API Functions (UPDATED) ---

pub async fn fetch_conversations() -> Result<Vec<Conversation>> {
    let client = Client::new();
    let resp = client.get(format!("{}/conversations", SEARCH_URL)).send().await?;
    Ok(resp.json::<Vec<Conversation>>().await?)
}

pub async fn load_conversation(id: i64) -> Result<Value> {
    let client = Client::new();
    let resp = client.get(format!("{}/conversations/{}", SEARCH_URL, id)).send().await?;
    Ok(resp.json::<Value>().await?)
}

pub async fn fetch_providers_list() -> Result<Vec<ProviderConfig>> {
    let client = Client::new();
    let resp = client.get(format!("{}/providers", SEARCH_URL)).send().await?;
    Ok(resp.json::<Vec<ProviderConfig>>().await?)
}

pub async fn fetch_models(provider: &str) -> Result<Vec<Model>> {
    let client = Client::new();
    let resp = client.get(format!("{}/models?provider={}", SEARCH_URL, provider)).send().await?;
    Ok(resp.json::<Vec<Model>>().await?)
}

pub async fn start_search_stream(
    query: String,
    convo_id: Option<i64>,
    model: String,
    provider: String,
    active_providers: Vec<i64>,
    tx: UnboundedSender<AppAction>
) -> Result<()> {
    let client = Client::new();

    // 1. Create or Use Conversation
    let id = if let Some(cid) = convo_id {
        cid
    } else {
        let convo_res = client.post(format!("{}/conversations", SEARCH_URL))
            .json(&serde_json::json!({ "title": query }))
            .send()
            .await?;
        let convo_json: Value = convo_res.json().await?;
        let new_id = convo_json["id"].as_i64().unwrap_or(1);
        tx.send(AppAction::ConversationCreated(new_id))?;
        new_id
    };

    // 2. Start Stream
    let body = serde_json::json!({
        "query": query,
        "timeframe": "", // Default all time
        "providers": active_providers,
        "provider": provider, 
        "model": model,
        "systemPrompt": "You are a helpful TUI assistant that provides concise markdown responses."
    });

    let mut stream = client
        .post(format!("{}/conversations/{}/query", SEARCH_URL, id))
        .json(&body)
        .send()
        .await?
        .bytes_stream()
        .eventsource();

    while let Some(event) = stream.next().await {
        match event {
            Ok(evt) => {
                match evt.event.as_str() {
                    "results" => {
                        if let Ok(sources) = serde_json::from_str::<Vec<SearchSource>>(&evt.data) {
                            let _ = tx.send(AppAction::SearchSourcesReceived(sources));
                        }
                    },
                    "summary-chunk" => {
                        if let Ok(data) = serde_json::from_str::<Value>(&evt.data) {
                            if let Some(text) = data["text"].as_str() {
                                let _ = tx.send(AppAction::SearchStreamToken(text.to_string()));
                            }
                        }
                    },
                    "error" => {
                        let _ = tx.send(AppAction::SearchError(evt.data));
                    },
                    "summary-done" => {
                        let _ = tx.send(AppAction::SearchDone);
                        break;
                    },
                    _ => {}
                }
            },
            Err(e) => {
                let _ = tx.send(AppAction::SearchError(e.to_string()));
                break;
            }
        }
    }

    Ok(())
}