use std::{collections::HashSet, path::Path, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use url::Url;

use crate::model::{Article, FoundSource, Source};

const SOURCE_COLUMNS: &str = "
    s.id, s.input_url, s.home_url, s.feed_url, s.title, s.kind, s.status,
    s.error, s.last_checked_at, s.last_success_at,
    (SELECT count(*) FROM articles a WHERE a.source_id = s.id AND a.is_read = 0)
";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating data directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS sources (
                 id INTEGER PRIMARY KEY,
                 input_url TEXT NOT NULL UNIQUE,
                 home_url TEXT NOT NULL,
                 feed_url TEXT,
                 title TEXT NOT NULL,
                 kind TEXT NOT NULL DEFAULT 'unknown',
                 status TEXT NOT NULL DEFAULT 'pending',
                 error TEXT,
                 last_checked_at TEXT,
                 last_success_at TEXT,
                 created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS articles (
                 id INTEGER PRIMARY KEY,
                 source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                 url TEXT NOT NULL,
                 title TEXT NOT NULL,
                 published_at TEXT,
                 position INTEGER NOT NULL,
                 discovered_at TEXT NOT NULL,
                 is_read INTEGER NOT NULL CHECK (is_read IN (0, 1)),
                 UNIQUE (source_id, url)
             );
             CREATE INDEX IF NOT EXISTS articles_unread_idx
                 ON articles(is_read, source_id, discovered_at);",
        )?;
        Ok(Self { conn })
    }

    pub fn add_source(&self, entered_url: &str) -> Result<i64> {
        let url = normalize_source_url(entered_url)?;
        let title = url.host_str().unwrap_or("Untitled blog");
        self.conn.execute(
            "INSERT INTO sources (input_url, home_url, title, created_at)
             VALUES (?1, ?1, ?2, ?3)",
            params![url.as_str(), title, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        let sql = format!("SELECT {SOURCE_COLUMNS} FROM sources s ORDER BY s.title COLLATE NOCASE");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], source_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn source(&self, id: i64) -> Result<Option<Source>> {
        let sql = format!("SELECT {SOURCE_COLUMNS} FROM sources s WHERE s.id = ?1");
        self.conn
            .query_row(&sql, [id], source_from_row)
            .optional()
            .map_err(Into::into)
    }

    pub fn articles_for(&self, source_id: i64) -> Result<Vec<Article>> {
        self.articles_where("WHERE a.source_id = ?1", &[&source_id])
    }

    pub fn unread_articles(&self) -> Result<Vec<Article>> {
        self.articles_where("WHERE a.is_read = 0", &[])
    }

    fn articles_where(&self, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<Article>> {
        let sql = format!(
            "SELECT a.id, a.source_id, s.title, a.url, a.title, a.published_at, a.is_read
             FROM articles a JOIN sources s ON s.id = a.source_id
             {filter}
             ORDER BY (a.published_at IS NULL) ASC, a.published_at DESC,
                      a.position ASC, a.discovered_at DESC, a.id ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(args, |row| {
            Ok(Article {
                id: row.get(0)?,
                source_id: row.get(1)?,
                source_title: row.get(2)?,
                url: row.get(3)?,
                title: row.get(4)?,
                published_at: row.get(5)?,
                is_read: row.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn set_read(&self, article_id: i64, read: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE articles SET is_read = ?1 WHERE id = ?2",
            params![read as i64, article_id],
        )?;
        Ok(())
    }

    /// `None` marks every article read; a source ID limits the operation.
    pub fn mark_all_read(&self, source_id: Option<i64>) -> Result<usize> {
        let changed = match source_id {
            Some(id) => self.conn.execute(
                "UPDATE articles SET is_read = 1 WHERE source_id = ?1 AND is_read = 0",
                [id],
            )?,
            None => self
                .conn
                .execute("UPDATE articles SET is_read = 1 WHERE is_read = 0", [])?,
        };
        Ok(changed)
    }

    pub fn delete_source(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sources WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn record_failure(&self, id: i64, status: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sources SET status = ?1, error = ?2, last_checked_at = ?3 WHERE id = ?4",
            params![status, error, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// Inserts unseen URLs and preserves read status for previously known URLs.
    pub fn record_success(&mut self, id: i64, found: FoundSource) -> Result<usize> {
        let first_success: bool = self.conn.query_row(
            "SELECT last_success_at IS NULL FROM sources WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let now = Utc::now().to_rfc3339();
        let mut articles = found.articles;
        let mut seen = HashSet::new();
        articles.retain(|article| seen.insert(article.url.clone()));
        let mut newest = articles.iter().collect::<Vec<_>>();
        newest.sort_by(|a, b| match (&a.published_at, &b.published_at) {
            (Some(a_date), Some(b_date)) => b_date.cmp(a_date).then(a.position.cmp(&b.position)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.position.cmp(&b.position),
        });
        let baseline_unread: HashSet<String> = newest
            .iter()
            .take(3)
            .map(|article| article.url.clone())
            .collect();

        let tx = self.conn.transaction()?;
        let mut inserted = 0;
        for article in articles {
            let is_read = first_success && !baseline_unread.contains(&article.url);
            inserted += tx.execute(
                "INSERT OR IGNORE INTO articles
                 (source_id, url, title, published_at, position, discovered_at, is_read)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    article.url,
                    article.title,
                    article.published_at,
                    article.position as i64,
                    now,
                    is_read as i64
                ],
            )?;
            tx.execute(
                "UPDATE articles SET title = ?1, published_at = COALESCE(?2, published_at),
                                    position = ?3
                 WHERE source_id = ?4 AND url = ?5",
                params![
                    article.title,
                    article.published_at,
                    article.position as i64,
                    id,
                    article.url
                ],
            )?;
        }
        tx.execute(
            "UPDATE sources SET home_url = ?1, feed_url = ?2, title = ?3, kind = ?4,
                                status = 'ok', error = NULL, last_checked_at = ?5,
                                last_success_at = ?5 WHERE id = ?6",
            params![
                found.home_url,
                found.feed_url,
                found.title,
                found.kind,
                now,
                id
            ],
        )?;
        tx.commit()?;
        Ok(inserted)
    }
}

pub fn normalize_source_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input.trim()).context("enter a complete http:// or https:// URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        anyhow::bail!("only public http:// and https:// URLs are supported");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("URLs with embedded credentials are not supported");
    }
    url.set_fragment(None);
    Ok(url)
}

fn source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get(0)?,
        input_url: row.get(1)?,
        home_url: row.get(2)?,
        feed_url: row.get(3)?,
        title: row.get(4)?,
        kind: row.get(5)?,
        status: row.get(6)?,
        error: row.get(7)?,
        last_checked_at: row.get(8)?,
        last_success_at: row.get(9)?,
        unread_count: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FoundArticle;

    fn found(count: usize) -> FoundSource {
        FoundSource {
            title: "Example".into(),
            home_url: "https://example.com/".into(),
            feed_url: None,
            kind: "html".into(),
            articles: (0..count)
                .map(|n| FoundArticle {
                    url: format!("https://example.com/{n}"),
                    title: format!("Post {n}"),
                    published_at: None,
                    position: n,
                })
                .collect(),
        }
    }

    #[test]
    fn first_check_marks_only_three_recent_articles_unread() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(file.path()).unwrap();
        let id = store.add_source("https://example.com/").unwrap();
        store.record_success(id, found(5)).unwrap();
        assert_eq!(store.source(id).unwrap().unwrap().unread_count, 3);
        store.record_success(id, found(5)).unwrap();
        assert_eq!(store.source(id).unwrap().unwrap().unread_count, 3);
        let mut next = found(6);
        next.articles.push(FoundArticle {
            url: "https://example.com/new".into(),
            title: "New".into(),
            published_at: None,
            position: 0,
        });
        store.record_success(id, next).unwrap();
        assert_eq!(store.source(id).unwrap().unwrap().unread_count, 5);
    }

    #[test]
    fn deleting_source_cascades_articles() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(file.path()).unwrap();
        let id = store.add_source("https://example.com/").unwrap();
        store.record_success(id, found(2)).unwrap();
        store.delete_source(id).unwrap();
        assert!(store.unread_articles().unwrap().is_empty());
    }

    #[test]
    fn publication_dates_override_feed_order_on_first_check() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(file.path()).unwrap();
        let id = store.add_source("https://example.com/").unwrap();
        let mut initial = found(4);
        for (article, date) in initial.articles.iter_mut().zip([
            "2026-01-01T00:00:00+00:00",
            "2026-04-01T00:00:00+00:00",
            "2026-03-01T00:00:00+00:00",
            "2026-02-01T00:00:00+00:00",
        ]) {
            article.published_at = Some(date.into());
        }
        store.record_success(id, initial).unwrap();
        let articles = store.articles_for(id).unwrap();
        assert!(
            articles
                .iter()
                .find(|article| article.url.ends_with("/0"))
                .unwrap()
                .is_read
        );
        assert_eq!(
            articles.iter().filter(|article| !article.is_read).count(),
            3
        );
    }

    #[test]
    fn failed_check_preserves_saved_articles() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(file.path()).unwrap();
        let id = store.add_source("https://example.com/").unwrap();
        store.record_success(id, found(2)).unwrap();
        store.record_failure(id, "error", "timeout").unwrap();
        let source = store.source(id).unwrap().unwrap();
        assert_eq!(source.status, "error");
        assert_eq!(source.unread_count, 2);
        assert!(source.last_success_at.is_some());
    }

    #[test]
    fn marking_an_article_read_keeps_its_position() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(file.path()).unwrap();
        let source_id = store.add_source("https://example.com/").unwrap();
        store.record_success(source_id, found(3)).unwrap();

        let before = store.articles_for(source_id).unwrap();
        assert_eq!(before.len(), 3);
        let first_id = before[0].id;
        let urls_before = before
            .iter()
            .map(|article| &article.url)
            .collect::<Vec<_>>();

        store.set_read(first_id, true).unwrap();

        let after = store.articles_for(source_id).unwrap();
        let urls_after = after.iter().map(|article| &article.url).collect::<Vec<_>>();
        assert_eq!(urls_after, urls_before);
        assert!(after[0].is_read);
    }
}
