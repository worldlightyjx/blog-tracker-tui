use std::{collections::HashSet, fmt, time::Duration};

use anyhow::{Context, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use feed_rs::model::Feed;
use regex::Regex;
use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::{
    config::{Config, SiteRule},
    model::{FoundArticle, FoundSource, Source},
};

#[derive(Debug)]
pub struct NeedsConfig(pub String);

impl fmt::Display for NeedsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NeedsConfig {}

pub struct Fetcher {
    client: Client,
}

impl Fetcher {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("blog-tracker-tui/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(8))
            .build()?;
        Ok(Self { client })
    }

    pub fn fetch(&self, source: &Source, config: &Config) -> Result<FoundSource> {
        let rule = config.rule_for(&source.input_url);
        if let Some(rule) = rule {
            return self.fetch_html(source, config, Some(rule), None);
        }

        if let Some(feed_url) = &source.feed_url {
            let (body, final_url) = self.get(feed_url)?;
            let feed = feed_rs::parser::parse(body.as_bytes())
                .with_context(|| format!("parsing feed {feed_url}"))?;
            return Ok(from_feed(feed, &final_url, Some(&source.home_url)));
        }

        let (body, final_url) = self.get(&source.input_url)?;
        if let Ok(feed) = feed_rs::parser::parse(body.as_bytes()) {
            return Ok(from_feed(feed, &final_url, None));
        }

        let document = Html::parse_document(&body);
        for candidate in feed_candidates(&document, &final_url) {
            if let Ok((feed_body, feed_final_url)) = self.get(candidate.as_str())
                && let Ok(feed) = feed_rs::parser::parse(feed_body.as_bytes())
            {
                return Ok(from_feed(feed, &feed_final_url, Some(final_url.as_str())));
            }
        }

        self.fetch_html(source, config, None, Some((body, final_url)))
    }

    fn get(&self, address: &str) -> Result<(String, Url)> {
        let response = self
            .client
            .get(address)
            .send()
            .with_context(|| format!("requesting {address}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error from {address}"))?;
        let final_url = response.url().clone();
        let body = response
            .text()
            .with_context(|| format!("reading {address}"))?;
        Ok((body, final_url))
    }

    fn fetch_html(
        &self,
        source: &Source,
        config: &Config,
        rule: Option<&SiteRule>,
        first_page: Option<(String, Url)>,
    ) -> Result<FoundSource> {
        let start_url = rule
            .and_then(|rule| rule.list_url.as_deref())
            .unwrap_or(&source.input_url);
        let max_pages = rule
            .and_then(|rule| rule.max_pages)
            .unwrap_or(config.max_pages);
        let mut next = Some(start_url.to_string());
        let mut first_page = first_page;
        let mut visited_pages = HashSet::new();
        let mut seen_articles = HashSet::new();
        let mut articles = Vec::new();
        let mut title = source.title.clone();
        let mut home_url = source.home_url.clone();

        for page_number in 0..max_pages {
            let Some(address) = next.take() else { break };
            if !visited_pages.insert(address.clone()) {
                break;
            }
            let (body, page_url) = match first_page.take() {
                Some(page) if page.1.as_str() == address => page,
                _ => self.get(&address)?,
            };
            let document = Html::parse_document(&body);
            if page_number == 0 {
                title = page_title(&document).unwrap_or_else(|| source.title.clone());
                if rule.is_none() {
                    home_url = page_url.as_str().to_string();
                }
            }
            let page_articles = html_articles(&document, &page_url, rule, articles.len())?;
            for article in page_articles {
                if seen_articles.insert(article.url.clone()) {
                    articles.push(article);
                }
            }
            next = next_page(&document, &page_url, rule)?;
        }

        if articles.is_empty() {
            return Err(NeedsConfig(format!(
                "No article links found at {start_url}; add a [[sites]] rule to the config file"
            ))
            .into());
        }
        Ok(FoundSource {
            title,
            home_url,
            feed_url: None,
            kind: "html".into(),
            articles,
        })
    }
}

fn from_feed(feed: Feed, feed_url: &Url, known_home: Option<&str>) -> FoundSource {
    let home = feed
        .links
        .iter()
        .find(|link| link.rel.as_deref().unwrap_or("alternate") == "alternate")
        .and_then(|link| feed_url.join(&link.href).ok())
        .map(|url| url.to_string())
        .or_else(|| known_home.map(str::to_string))
        .unwrap_or_else(|| origin(feed_url).to_string());
    let title = feed
        .title
        .map(|text| text.content)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| feed_url.host_str().unwrap_or("Blog").to_string());
    let articles = feed
        .entries
        .into_iter()
        .enumerate()
        .filter_map(|(position, entry)| {
            let link = entry
                .links
                .iter()
                .find(|link| link.rel.as_deref().unwrap_or("alternate") == "alternate")
                .or_else(|| entry.links.first())?;
            let mut url = feed_url.join(&link.href).ok()?;
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            url.set_fragment(None);
            let title = entry
                .title
                .map(|text| text.content)
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| url.as_str().to_string());
            Some(FoundArticle {
                url: url.to_string(),
                title: clean_text(&title),
                published_at: entry
                    .published
                    .or(entry.updated)
                    .map(|date| date.to_rfc3339()),
                position,
            })
        })
        .collect();
    FoundSource {
        title: clean_text(&title),
        home_url: home,
        feed_url: Some(feed_url.to_string()),
        kind: "feed".into(),
        articles,
    }
}

fn feed_candidates(document: &Html, page_url: &Url) -> Vec<Url> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let link_selector = selector("link[rel][href]");
    for link in document.select(&link_selector) {
        let rel = link.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        let media_type = link.value().attr("type").unwrap_or("").to_ascii_lowercase();
        if rel.split_whitespace().any(|part| part == "alternate")
            && (media_type.contains("rss")
                || media_type.contains("atom")
                || media_type.contains("feed+json"))
            && let Some(url) = link
                .value()
                .attr("href")
                .and_then(|href| page_url.join(href).ok())
            && seen.insert(url.to_string())
        {
            candidates.push(url);
        }
    }
    let anchor_selector = selector("a[href]");
    for anchor in document.select(&anchor_selector) {
        let href = anchor.value().attr("href").unwrap_or("");
        let text = text_of(anchor).to_ascii_lowercase();
        if (href.contains("feed") || href.contains("rss") || href.contains("atom"))
            && (text.contains("rss")
                || text.contains("feed")
                || href.ends_with(".xml")
                || href.ends_with("/feed/"))
            && let Ok(url) = page_url.join(href)
            && url.host_str() == page_url.host_str()
            && seen.insert(url.to_string())
        {
            candidates.push(url);
        }
    }
    for base in [page_url.clone(), origin(page_url)] {
        for path in ["feed.xml", "feed/", "rss.xml", "atom.xml"] {
            if let Ok(url) = base.join(path)
                && seen.insert(url.to_string())
            {
                candidates.push(url);
            }
        }
    }
    candidates
}

fn html_articles(
    document: &Html,
    page_url: &Url,
    rule: Option<&SiteRule>,
    start_position: usize,
) -> Result<Vec<FoundArticle>> {
    let mut articles = Vec::new();
    let mut seen = HashSet::new();
    if let Some(rule) = rule {
        let custom = Selector::parse(&rule.article_selector)
            .map_err(|_| NeedsConfig(format!("Invalid article_selector for {}", rule.url)))?;
        for item in document.select(&custom) {
            let anchor = if item.value().name() == "a" {
                Some(item)
            } else {
                item.select(&selector("a[href]")).next()
            };
            if let Some(anchor) = anchor
                && let Some(mut article) =
                    article_from_anchor(anchor, page_url, articles.len() + start_position)
                && seen.insert(article.url.clone())
            {
                article.published_at = item
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|ancestor| ancestor.value().name() == "article")
                    .and_then(date_in)
                    .or_else(|| date_in(item));
                articles.push(article);
            }
        }
        return Ok(articles);
    }

    let containers = selector("article, .post-preview, .post-item, .post-list-item, .entry");
    let preferred = selector(
        "h1 a[href], h2 a[href], h3 a[href], a.post-title[href], a.entry-title[href], a[href]:has(h1), a[href]:has(h2), a[href]:has(h3)",
    );
    let any_anchor = selector("a[href]");
    for container in document.select(&containers) {
        let anchor = container.select(&preferred).next().or_else(|| {
            container.select(&any_anchor).find(|link| {
                link.value()
                    .attr("href")
                    .and_then(|href| page_url.join(href).ok())
                    .is_some_and(|url| url.host_str() == page_url.host_str())
            })
        });
        if let Some(anchor) = anchor
            && let Some(mut article) =
                article_from_anchor(anchor, page_url, articles.len() + start_position)
            && seen.insert(article.url.clone())
        {
            article.published_at = date_in(container);
            articles.push(article);
        }
    }
    if !articles.is_empty() {
        return Ok(articles);
    }

    let headings = selector(
        "h1 a[href], h2 a[href], h3 a[href], a.post-title[href], a.entry-title[href], a[href]:has(h1), a[href]:has(h2), a[href]:has(h3)",
    );
    for anchor in document.select(&headings) {
        if let Some(article) =
            article_from_anchor(anchor, page_url, articles.len() + start_position)
            && seen.insert(article.url.clone())
        {
            articles.push(article);
        }
    }
    Ok(articles)
}

fn article_from_anchor(
    anchor: ElementRef<'_>,
    page_url: &Url,
    position: usize,
) -> Option<FoundArticle> {
    let href = anchor.value().attr("href")?;
    let mut url = page_url.join(href).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str() != page_url.host_str() {
        return None;
    }
    url.set_fragment(None);
    if url == *page_url || is_non_article_path(&url) {
        return None;
    }
    let title = text_of(anchor);
    if title.chars().count() < 3 {
        return None;
    }
    Some(FoundArticle {
        url: url.to_string(),
        title,
        published_at: None,
        position,
    })
}

fn is_non_article_path(url: &Url) -> bool {
    let path = url.path().trim_matches('/').to_ascii_lowercase();
    if path.is_empty() {
        return true;
    }
    let first = path.split('/').next().unwrap_or("");
    matches!(
        first,
        "about"
            | "contact"
            | "tags"
            | "tag"
            | "categories"
            | "category"
            | "archives"
            | "archive"
            | "feed"
            | "rss"
            | "search"
            | "page"
    ) || first.starts_with("page") && first[4..].chars().all(|ch| ch.is_ascii_digit())
        || path.ends_with(".xml")
}

fn next_page(document: &Html, page_url: &Url, rule: Option<&SiteRule>) -> Result<Option<String>> {
    if let Some(custom) = rule.and_then(|rule| rule.next_page_selector.as_deref()) {
        let selector = Selector::parse(custom).map_err(|_| {
            NeedsConfig(format!(
                "Invalid next_page_selector for {}",
                rule.unwrap().url
            ))
        })?;
        let next = document
            .select(&selector)
            .next()
            .and_then(|element| element.value().attr("href"))
            .and_then(|href| page_url.join(href).ok())
            .filter(|url| url.host_str() == page_url.host_str());
        return Ok(next.map(|url| url.to_string()));
    }
    for anchor in document.select(&selector("a[href]")) {
        let rel = anchor
            .value()
            .attr("rel")
            .unwrap_or("")
            .to_ascii_lowercase();
        let text = text_of(anchor).to_ascii_lowercase();
        if let Some(href) = anchor.value().attr("href")
            && let Ok(url) = page_url.join(href)
            && url.host_str() == page_url.host_str()
            && url != *page_url
        {
            let rel_next = rel.split_whitespace().any(|part| part == "next");
            let label_next = ["older", "next", "下一页", "较旧", "后页", "»"]
                .iter()
                .any(|word| text.contains(word));
            let numeric_next = text.trim().parse::<usize>().ok() == Some(page_number(page_url) + 1);
            let inside_pagination =
                anchor
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .any(|element| {
                        element.value().attr("class").is_some_and(|classes| {
                            classes.contains("pagination") || classes.contains("pager")
                        })
                    });
            if rel_next
                || (label_next
                    && (looks_like_page_url(&url) || inside_pagination || text == "较旧"))
                || (numeric_next && looks_like_page_url(&url))
            {
                return Ok(Some(url.to_string()));
            }
        }
    }
    Ok(None)
}

fn looks_like_page_url(url: &Url) -> bool {
    let segments = url
        .path_segments()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    segments
        .windows(2)
        .any(|pair| pair[0] == "page" && pair[1].parse::<usize>().is_ok())
        || segments.iter().any(|segment| {
            segment
                .strip_prefix("page")
                .is_some_and(|number| !number.is_empty() && number.parse::<usize>().is_ok())
        })
        || url.query_pairs().any(|(key, value)| {
            matches!(key.as_ref(), "page" | "paged") && value.parse::<usize>().is_ok()
        })
}

fn page_number(url: &Url) -> usize {
    let segments = url
        .path_segments()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for pair in segments.windows(2) {
        if pair[0] == "page"
            && let Ok(number) = pair[1].parse()
        {
            return number;
        }
    }
    for segment in segments {
        if let Some(number) = segment.strip_prefix("page")
            && let Ok(number) = number.parse()
        {
            return number;
        }
    }
    url.query_pairs()
        .find_map(|(key, value)| {
            if matches!(key.as_ref(), "page" | "paged") {
                value.parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(1)
}

fn date_in(container: ElementRef<'_>) -> Option<String> {
    let time = container.select(&selector("time")).next();
    let raw = time
        .as_ref()
        .and_then(|element| element.value().attr("datetime"))
        .map(str::to_string)
        .or_else(|| time.map(text_of))
        .unwrap_or_else(|| text_of(container));
    parse_date(&raw)
}

fn parse_date(raw: &str) -> Option<String> {
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(raw.trim()) {
        return Some(date.with_timezone(&Utc).to_rfc3339());
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(raw.trim()) {
        return Some(date.with_timezone(&Utc).to_rfc3339());
    }
    let re = Regex::new(r"\b\d{4}[-/]\d{1,2}[-/]\d{1,2}\b").ok()?;
    if let Some(matched) = re.find(raw) {
        let date = matched.as_str().replace('/', "-");
        if let Ok(date) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
            return Utc
                .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
                .single()
                .map(|date| date.to_rfc3339());
        }
    }
    None
}

fn page_title(document: &Html) -> Option<String> {
    document
        .select(&selector("title"))
        .next()
        .map(text_of)
        .filter(|title| !title.is_empty())
}

fn text_of(element: ElementRef<'_>) -> String {
    clean_text(&element.text().collect::<Vec<_>>().join(" "))
}

fn clean_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn selector(raw: &str) -> Selector {
    Selector::parse(raw).expect("built-in CSS selector is valid")
}

fn origin(url: &Url) -> Url {
    let mut origin = url.clone();
    origin.set_path("/");
    origin.set_query(None);
    origin.set_fragment(None);
    origin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_fallback_ignores_navigation_and_follows_older_page() {
        let page = Url::parse("https://example.com/").unwrap();
        let document = Html::parse_document(
            "<title>Example</title><nav><a href='/about'>About</a></nav><article><a href='/posts/one'><h2>First post</h2></a><time datetime='2026-01-01'>Jan 1</time></article><a href='/page/2/'>Older posts »</a>",
        );
        let articles = html_articles(&document, &page, None, 0).unwrap();
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].url, "https://example.com/posts/one");
        assert_eq!(
            next_page(&document, &page, None).unwrap().as_deref(),
            Some("https://example.com/page/2/")
        );
    }

    #[test]
    fn manual_selector_controls_article_links() {
        let page = Url::parse("https://example.com/").unwrap();
        let document = Html::parse_document(
            "<a href='/about'>About</a><div class='entry'><a href='/a'>Article A</a></div>",
        );
        let rule = SiteRule {
            url: page.to_string(),
            list_url: None,
            article_selector: ".entry a".into(),
            next_page_selector: None,
            max_pages: None,
        };
        let articles = html_articles(&document, &page, Some(&rule), 0).unwrap();
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Article A");
    }

    #[test]
    fn numbered_pagination_uses_the_next_page() {
        let page = Url::parse("https://example.com/page/2/").unwrap();
        let document = Html::parse_document(
            "<nav class='pagination'><a href='/page/1/'>1</a><a href='/page/3/'>3</a></nav>",
        );
        assert_eq!(
            next_page(&document, &page, None).unwrap().as_deref(),
            Some("https://example.com/page/3/")
        );
    }
}
