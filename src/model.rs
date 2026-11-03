#[derive(Debug, Clone)]
pub struct Source {
    pub id: i64,
    pub input_url: String,
    pub home_url: String,
    pub feed_url: Option<String>,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub error: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub unread_count: i64,
}

#[derive(Debug, Clone)]
pub struct Article {
    pub id: i64,
    pub source_id: i64,
    pub source_title: String,
    pub url: String,
    pub title: String,
    pub published_at: Option<String>,
    pub is_read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundArticle {
    pub url: String,
    pub title: String,
    pub published_at: Option<String>,
    /// Position in the source's listing. Lower values are newer when dates are absent.
    pub position: usize,
}

#[derive(Debug, Clone)]
pub struct FoundSource {
    pub title: String,
    pub home_url: String,
    pub feed_url: Option<String>,
    pub kind: String,
    pub articles: Vec<FoundArticle>,
}
