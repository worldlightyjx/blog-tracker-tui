use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::theme::ThemeConfig;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub check_interval_minutes: u64,
    pub max_pages: usize,
    pub sites: Vec<SiteRule>,
    pub theme: ThemeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            check_interval_minutes: 30,
            max_pages: 3,
            sites: Vec::new(),
            theme: ThemeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SiteRule {
    /// Must match the URL entered when the source was added (trailing slash ignored).
    pub url: String,
    pub list_url: Option<String>,
    /// CSS selector for article links, for example `article h2 a[href]`.
    pub article_selector: String,
    pub next_page_selector: Option<String>,
    pub max_pages: Option<usize>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)
            .with_context(|| format!("reading configuration {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing configuration {}", path.display()))?;
        if config.check_interval_minutes == 0 || config.max_pages == 0 {
            anyhow::bail!("check_interval_minutes and max_pages must be greater than zero");
        }
        for rule in &config.sites {
            if rule.max_pages == Some(0) {
                anyhow::bail!("max_pages for {} must be greater than zero", rule.url);
            }
        }
        Ok(config)
    }

    pub fn rule_for(&self, input_url: &str) -> Option<&SiteRule> {
        self.sites
            .iter()
            .find(|rule| rule.url.trim_end_matches('/') == input_url.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_manual_rule_ignoring_trailing_slash() {
        let config: Config = toml::from_str(
            "[[sites]]\nurl = 'https://example.com/'\narticle_selector = 'article h2 a'",
        )
        .unwrap();
        assert!(config.rule_for("https://example.com").is_some());
    }

    #[test]
    fn theme_and_site_rules_can_share_one_config_file() {
        let config: Config = toml::from_str(
            "check_interval_minutes = 30\nmax_pages = 3\n[theme]\nselected_fg = '#ffffff'\nselected_bg = '#1e40af'\n[[sites]]\nurl = 'https://example.com/'\narticle_selector = 'article a[href]'",
        )
        .unwrap();
        assert!(config.theme.resolve().1.is_empty());
        assert!(config.rule_for("https://example.com/").is_some());
    }
}
