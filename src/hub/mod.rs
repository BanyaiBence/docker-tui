pub mod models;

use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use models::{Repo, SearchPage, TagPage};

pub const PAGE_SIZE: u32 = 25;
const BASE: &str = "https://hub.docker.com/v2";

/// Read-only access to Docker Hub. Mocked in tests.
#[async_trait]
pub trait HubApi: Send + Sync {
    async fn search(&self, query: &str, page: u32) -> Result<SearchPage, String>;
    async fn tags(&self, repo: &Repo, page: u32) -> Result<TagPage, String>;
}

pub fn search_url(query: &str, page: u32) -> String {
    format!(
        "{BASE}/search/repositories/?query={}&page={page}&page_size={PAGE_SIZE}",
        percent_encode(query)
    )
}

pub fn tags_url(repo: &Repo, page: u32) -> String {
    let (ns, name) = repo.namespace_and_name();
    format!(
        "{BASE}/repositories/{}/{}/tags?page={page}&page_size={PAGE_SIZE}&ordering=last_updated",
        percent_encode(ns),
        percent_encode(name)
    )
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub struct ReqwestHub {
    client: reqwest::Client,
    /// Response bodies by URL; Hub rate-limits anonymous clients.
    cache: Mutex<HashMap<String, String>>,
}

impl ReqwestHub {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("docker-tui/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Self {
            client,
            cache: Mutex::default(),
        }
    }

    async fn get(&self, url: &str) -> Result<String, String> {
        if let Some(body) = self.cache.lock().unwrap().get(url) {
            return Ok(body.clone());
        }
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("Docker Hub rate limit reached, try again later".into());
        }
        if !status.is_success() {
            return Err(format!("Docker Hub returned {status}"));
        }
        let body = resp.text().await.map_err(|e| e.to_string())?;
        self.cache
            .lock()
            .unwrap()
            .insert(url.to_string(), body.clone());
        Ok(body)
    }
}

impl Default for ReqwestHub {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HubApi for ReqwestHub {
    async fn search(&self, query: &str, page: u32) -> Result<SearchPage, String> {
        models::parse_search(&self.get(&search_url(query, page)).await?)
    }

    async fn tags(&self, repo: &Repo, page: u32) -> Result<TagPage, String> {
        models::parse_tags(&self.get(&tags_url(repo, page)).await?)
    }
}
