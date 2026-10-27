use async_trait::async_trait;

use docker_tui::hub::{
    HubApi,
    models::{Repo, SearchPage, Tag, TagPage},
};

#[derive(Default)]
pub struct MockHub;

#[async_trait]
impl HubApi for MockHub {
    async fn search(&self, query: &str, page: u32) -> Result<SearchPage, String> {
        if query == "fail" {
            return Err("rate limit".into());
        }
        let repo = Repo {
            name: format!("{query}-{page}"),
            description: String::new(),
            stars: 0,
            pulls: 0,
            official: true,
        };
        Ok(SearchPage {
            count: 1,
            results: vec![repo],
            has_next: false,
        })
    }

    async fn tags(&self, repo: &Repo, _page: u32) -> Result<TagPage, String> {
        let tag = Tag {
            name: format!("{}-tag", repo.name),
            size: 0,
            last_updated: None,
            architectures: vec![],
        };
        Ok(TagPage {
            count: 1,
            results: vec![tag],
            has_next: false,
        })
    }
}
