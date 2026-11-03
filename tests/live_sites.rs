//! Network acceptance checks for the user's four example sites.
//! Run explicitly with: cargo test --test live_sites -- --ignored --nocapture

use blog_tracker_tui::{
    config::{Config, SiteRule},
    fetch::Fetcher,
    model::Source,
};

fn source(url: &str) -> Source {
    Source {
        id: 1,
        input_url: url.into(),
        home_url: url.into(),
        feed_url: None,
        title: url.into(),
        kind: "unknown".into(),
        status: "pending".into(),
        error: None,
        last_checked_at: None,
        last_success_at: None,
        unread_count: 0,
    }
}

#[test]
#[ignore = "requires live network access"]
fn four_example_blogs_have_article_links() {
    let fetcher = Fetcher::new().unwrap();
    let config = Config::default();
    for url in [
        "https://bitsflow.org/",
        "https://tw93.fun/",
        "https://www.kawabangga.com/",
        "https://dev.37signals.com/",
    ] {
        let found = fetcher
            .fetch(&source(url), &config)
            .unwrap_or_else(|error| {
                panic!("could not fetch {url}: {error:#}");
            });
        assert!(!found.articles.is_empty(), "{url} returned no articles");
        assert!(
            found
                .articles
                .iter()
                .all(|article| article.url.starts_with("http")),
            "{url} returned a non-browser URL"
        );
        eprintln!(
            "{url}: {} {} article(s), feed {:?}",
            found.kind,
            found.articles.len(),
            found.feed_url
        );
    }
}

#[test]
#[ignore = "requires live network access"]
fn bitsflow_can_be_tracked_from_html_with_a_manual_rule() {
    let fetcher = Fetcher::new().unwrap();
    let url = "https://bitsflow.org/";
    let config = Config {
        sites: vec![SiteRule {
            url: url.into(),
            list_url: None,
            article_selector: "article a[href]".into(),
            next_page_selector: None,
            max_pages: Some(2),
        }],
        ..Config::default()
    };
    let found = fetcher.fetch(&source(url), &config).unwrap();
    eprintln!(
        "bitsflow HTML: {} article(s), first: {:?}",
        found.articles.len(),
        found.articles.first()
    );
    assert_eq!(found.kind, "html");
    assert!(
        found.articles.len() > 10,
        "expected a second page of articles"
    );
    assert!(found.articles[0].published_at.is_some());
}
