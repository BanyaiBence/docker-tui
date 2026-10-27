//! Docker Hub API response models.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// `nginx` for official images, `owner/name` otherwise.
    pub name: String,
    pub description: String,
    pub stars: u64,
    pub pulls: u64,
    pub official: bool,
}

impl Repo {
    /// Namespace and repository for the tags endpoint (`library` for official images).
    pub fn namespace_and_name(&self) -> (&str, &str) {
        self.name.split_once('/').unwrap_or(("library", &self.name))
    }

    /// Image reference for pulling `tag`.
    pub fn image_ref(&self, tag: &str) -> String {
        format!("{}:{tag}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchPage {
    pub count: u64,
    pub results: Vec<Repo>,
    pub has_next: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub size: u64,
    /// RFC 3339 timestamp.
    pub last_updated: Option<String>,
    pub architectures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagPage {
    pub count: u64,
    pub results: Vec<Tag>,
    pub has_next: bool,
}

// Raw wire formats. Hub sends `null` for many fields, so everything is optional.

#[derive(Deserialize)]
struct RawPage<T> {
    count: Option<u64>,
    next: Option<String>,
    results: Option<Vec<T>>,
}

impl<T> RawPage<T> {
    fn has_next(&self) -> bool {
        self.next.as_deref().is_some_and(|n| !n.is_empty())
    }
}

#[derive(Deserialize)]
struct RawRepo {
    repo_name: Option<String>,
    short_description: Option<String>,
    star_count: Option<u64>,
    pull_count: Option<u64>,
    is_official: Option<bool>,
}

#[derive(Deserialize)]
struct RawTag {
    name: Option<String>,
    full_size: Option<u64>,
    last_updated: Option<String>,
    images: Option<Vec<RawTagImage>>,
}

#[derive(Deserialize)]
struct RawTagImage {
    architecture: Option<String>,
}

pub fn parse_search(json: &str) -> Result<SearchPage, String> {
    let raw: RawPage<RawRepo> =
        serde_json::from_str(json).map_err(|e| format!("unexpected Hub response: {e}"))?;
    let has_next = raw.has_next();
    let results = raw
        .results
        .unwrap_or_default()
        .into_iter()
        .map(|r| Repo {
            name: r.repo_name.unwrap_or_default(),
            description: r.short_description.unwrap_or_default(),
            stars: r.star_count.unwrap_or_default(),
            pulls: r.pull_count.unwrap_or_default(),
            official: r.is_official.unwrap_or_default(),
        })
        .collect();
    Ok(SearchPage {
        count: raw.count.unwrap_or_default(),
        results,
        has_next,
    })
}

pub fn parse_tags(json: &str) -> Result<TagPage, String> {
    let raw: RawPage<RawTag> =
        serde_json::from_str(json).map_err(|e| format!("unexpected Hub response: {e}"))?;
    let has_next = raw.has_next();
    let results = raw
        .results
        .unwrap_or_default()
        .into_iter()
        .map(|t| {
            let mut architectures: Vec<String> = Vec::new();
            for arch in t
                .images
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| i.architecture)
            {
                if arch != "unknown" && !architectures.contains(&arch) {
                    architectures.push(arch);
                }
            }
            Tag {
                name: t.name.unwrap_or_default(),
                size: t.full_size.unwrap_or_default(),
                last_updated: t.last_updated,
                architectures,
            }
        })
        .collect();
    Ok(TagPage {
        count: raw.count.unwrap_or_default(),
        results,
        has_next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../tests/fixtures/hub_search.json");
    const TAGS: &str = include_str!("../../tests/fixtures/hub_tags.json");

    fn repo(name: &str, official: bool) -> Repo {
        Repo {
            name: name.into(),
            description: String::new(),
            stars: 0,
            pulls: 0,
            official,
        }
    }

    #[test]
    fn parses_search_results() {
        let page = parse_search(SEARCH).unwrap();
        assert!(page.count > 1000);
        assert!(page.has_next);
        assert_eq!(page.results.len(), 3);
        let nginx = &page.results[0];
        assert_eq!(nginx.name, "nginx");
        assert!(nginx.official);
        assert!(nginx.stars > 0 && nginx.pulls > 0);
        assert_eq!(nginx.description, "Official build of Nginx.");
        assert_eq!(page.results[1].name, "nginx/nginx-ingress");
    }

    #[test]
    fn parses_tags() {
        let page = parse_tags(TAGS).unwrap();
        assert!(page.has_next);
        assert_eq!(page.results.len(), 3);
        let t = &page.results[0];
        assert_eq!(t.name, "stable-alpine3.24-slim");
        assert_eq!(t.size, 5_740_963);
        assert_eq!(t.architectures, ["amd64", "ppc64le"]);
        assert!(t.last_updated.as_deref().unwrap().starts_with("2026-"));
    }

    #[test]
    fn last_page_has_no_next() {
        let page = parse_search(r#"{"count": 1, "next": "", "results": []}"#).unwrap();
        assert!(!page.has_next);
        let page = parse_tags(r#"{"count": 1, "next": null, "results": []}"#).unwrap();
        assert!(!page.has_next);
    }

    #[test]
    fn tolerates_missing_and_null_fields() {
        let page = parse_search(r#"{"results": [{"repo_name": "x", "short_description": null}]}"#)
            .unwrap();
        assert_eq!(page.results[0].name, "x");
        let page = parse_tags(r#"{"results": [{"name": "latest", "images": null}]}"#).unwrap();
        assert_eq!(page.results[0].architectures, Vec::<String>::new());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_search("<html>").is_err());
    }

    #[test]
    fn architectures_are_deduplicated_and_skip_unknown() {
        let json = r#"{"results": [{"name": "t", "images": [
            {"architecture": "amd64"}, {"architecture": "amd64"}, {"architecture": "unknown"}, {"architecture": "arm64"}
        ]}]}"#;
        assert_eq!(
            parse_tags(json).unwrap().results[0].architectures,
            ["amd64", "arm64"]
        );
    }

    #[test]
    fn official_repos_live_in_library() {
        assert_eq!(
            repo("nginx", true).namespace_and_name(),
            ("library", "nginx")
        );
        assert_eq!(
            repo("bitnami/redis", false).namespace_and_name(),
            ("bitnami", "redis")
        );
    }

    #[test]
    fn image_ref_includes_tag() {
        assert_eq!(repo("nginx", true).image_ref("1.27"), "nginx:1.27");
        assert_eq!(
            repo("bitnami/redis", false).image_ref("latest"),
            "bitnami/redis:latest"
        );
    }
}
