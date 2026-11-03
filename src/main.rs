use std::{
    io::{self, Stdout},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use blog_tracker_tui::{
    config::Config,
    db::Store,
    fetch::{Fetcher, NeedsConfig},
    model::{Article, Source},
};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

#[derive(Parser)]
#[command(
    version,
    about = "Track new blog posts from feeds and public web pages"
)]
struct Args {
    /// Override the SQLite data directory.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Override the TOML configuration file.
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum View {
    Sources,
    Articles(i64),
    Unread,
}

enum Dialog {
    None,
    Add(String),
    ConfirmDelete(i64, String),
    Help,
}

enum WorkerEvent {
    Updated(i64, usize),
    Failed(i64, String),
    Finished,
}

struct App {
    store: Store,
    db_path: PathBuf,
    config_path: PathBuf,
    sources: Vec<Source>,
    articles: Vec<Article>,
    view: View,
    selected: usize,
    dialog: Dialog,
    message: String,
    checking: bool,
    pending_refresh: Vec<i64>,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    next_check: Instant,
    quit: bool,
}

impl App {
    fn new(db_path: PathBuf, config_path: PathBuf) -> Result<Self> {
        let config = Config::load(&config_path)?;
        let store = Store::open(&db_path)?;
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            store,
            db_path,
            config_path,
            sources: Vec::new(),
            articles: Vec::new(),
            view: View::Sources,
            selected: 0,
            dialog: Dialog::None,
            message: "Press ? for help. Add a blog with a.".into(),
            checking: false,
            pending_refresh: Vec::new(),
            tx,
            rx,
            next_check: Instant::now()
                + Duration::from_secs(config.check_interval_minutes.saturating_mul(60)),
            quit: false,
        };
        app.reload()?;
        app.refresh(None);
        Ok(app)
    }

    fn reload(&mut self) -> Result<()> {
        self.sources = self.store.sources()?;
        self.articles = match self.view {
            View::Sources => Vec::new(),
            View::Articles(source_id) => self.store.articles_for(source_id)?,
            View::Unread => self.store.unread_articles()?,
        };
        let len = match self.view {
            View::Sources => self.sources.len(),
            _ => self.articles.len(),
        };
        self.selected = self.selected.min(len.saturating_sub(1));
        Ok(())
    }

    fn refresh(&mut self, ids: Option<Vec<i64>>) {
        if self.checking {
            if let Some(ids) = ids {
                self.pending_refresh.extend(ids);
            }
            return;
        }
        let db_path = self.db_path.clone();
        let config_path = self.config_path.clone();
        let tx = self.tx.clone();
        self.checking = true;
        self.message = "Checking blogs in the background…".into();
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                let config = Config::load(&config_path)?;
                let mut store = Store::open(&db_path)?;
                let fetcher = Fetcher::new()?;
                let ids = match ids {
                    Some(ids) => ids,
                    None => store
                        .sources()?
                        .into_iter()
                        .map(|source| source.id)
                        .collect(),
                };
                for id in ids {
                    let Some(source) = store.source(id)? else {
                        continue;
                    };
                    match fetcher.fetch(&source, &config) {
                        Ok(found) => match store.record_success(id, found) {
                            Ok(added) => {
                                let _ = tx.send(WorkerEvent::Updated(id, added));
                            }
                            Err(error) => {
                                let _ = tx.send(WorkerEvent::Failed(id, error.to_string()));
                            }
                        },
                        Err(error) => {
                            let status = if error.downcast_ref::<NeedsConfig>().is_some() {
                                "needs_config"
                            } else {
                                "error"
                            };
                            let detail = format!("{error:#}");
                            if let Err(save_error) = store.record_failure(id, status, &detail) {
                                let _ = tx.send(WorkerEvent::Failed(id, save_error.to_string()));
                            } else {
                                let _ = tx.send(WorkerEvent::Failed(id, detail));
                            }
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = tx.send(WorkerEvent::Failed(0, format!("{error:#}")));
            }
            let _ = tx.send(WorkerEvent::Finished);
        });
    }

    fn tick(&mut self) -> Result<()> {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::Updated(id, added) => {
                    self.reload()?;
                    let name = self
                        .sources
                        .iter()
                        .find(|source| source.id == id)
                        .map(|source| source.title.as_str())
                        .unwrap_or("Blog");
                    self.message = format!("Checked {name}: {added} new saved article(s)");
                }
                WorkerEvent::Failed(id, error) => {
                    self.reload()?;
                    let name = self
                        .sources
                        .iter()
                        .find(|source| source.id == id)
                        .map(|source| source.title.as_str())
                        .unwrap_or("Check");
                    self.message = format!("{name}: {error}");
                }
                WorkerEvent::Finished => {
                    self.checking = false;
                    if self.message == "Checking blogs in the background…" {
                        self.message = "Check complete".into();
                    }
                    if !self.pending_refresh.is_empty() {
                        let ids = std::mem::take(&mut self.pending_refresh);
                        self.refresh(Some(ids));
                    }
                }
            }
        }
        if Instant::now() >= self.next_check {
            match Config::load(&self.config_path) {
                Ok(config) => {
                    self.next_check = Instant::now()
                        + Duration::from_secs(config.check_interval_minutes.saturating_mul(60));
                    self.refresh(None);
                }
                Err(error) => {
                    self.message = format!("Config error: {error:#}");
                    self.next_check = Instant::now() + Duration::from_secs(60);
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }
        match &mut self.dialog {
            Dialog::Add(input) => match key.code {
                KeyCode::Esc => self.dialog = Dialog::None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    let entered = input.clone();
                    self.dialog = Dialog::None;
                    match self.store.add_source(&entered) {
                        Ok(id) => {
                            self.view = View::Sources;
                            self.reload()?;
                            self.selected = self
                                .sources
                                .iter()
                                .position(|source| source.id == id)
                                .unwrap_or(0);
                            self.refresh(Some(vec![id]));
                        }
                        Err(error) => self.message = format!("Could not add blog: {error:#}"),
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    input.push(ch)
                }
                _ => {}
            },
            Dialog::ConfirmDelete(id, _) => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let id = *id;
                    self.store.delete_source(id)?;
                    self.dialog = Dialog::None;
                    self.view = View::Sources;
                    self.reload()?;
                    self.message = "Blog and its local article history deleted".into();
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.dialog = Dialog::None;
                }
                _ => {}
            },
            Dialog::Help => self.dialog = Dialog::None,
            Dialog::None => self.handle_normal_key(key)?,
        }
        Ok(())
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => match self.view {
                View::Sources => self.quit = true,
                _ => {
                    self.view = View::Sources;
                    self.selected = 0;
                    self.reload()?;
                }
            },
            KeyCode::Char('?') => self.dialog = Dialog::Help,
            KeyCode::Char('a') => self.dialog = Dialog::Add(String::new()),
            KeyCode::Char('r') => self.refresh(None),
            KeyCode::Char('u') => {
                self.view = View::Unread;
                self.selected = 0;
                self.reload()?;
            }
            KeyCode::Char('s') => {
                self.view = View::Sources;
                self.selected = 0;
                self.reload()?;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = match self.view {
                    View::Sources => self.sources.len(),
                    _ => self.articles.len(),
                };
                self.selected = (self.selected + 1).min(len.saturating_sub(1));
            }
            KeyCode::Enter => self.activate_selected()?,
            KeyCode::Char('h') => self.open_homepage()?,
            KeyCode::Char(' ') => self.toggle_read()?,
            KeyCode::Char('m') => self.mark_all_read()?,
            KeyCode::Char('d') => self.confirm_delete(),
            _ => {}
        }
        Ok(())
    }

    fn activate_selected(&mut self) -> Result<()> {
        match self.view {
            View::Sources => {
                if let Some(source) = self.sources.get(self.selected) {
                    self.view = View::Articles(source.id);
                    self.selected = 0;
                    self.reload()?;
                }
            }
            View::Articles(_) | View::Unread => {
                if let Some(article) = self.articles.get(self.selected) {
                    match webbrowser::open(&article.url) {
                        Ok(()) => {
                            self.store.set_read(article.id, true)?;
                            self.message = format!("Opened {}", article.title);
                            self.reload()?;
                        }
                        Err(error) => self.message = format!("Could not open browser: {error}"),
                    }
                }
            }
        }
        Ok(())
    }

    fn open_homepage(&mut self) -> Result<()> {
        let source = match self.view {
            View::Sources => self.sources.get(self.selected),
            View::Articles(id) => self.sources.iter().find(|source| source.id == id),
            View::Unread => self.articles.get(self.selected).and_then(|article| {
                self.sources
                    .iter()
                    .find(|source| source.id == article.source_id)
            }),
        };
        if let Some(source) = source {
            match webbrowser::open(&source.home_url) {
                Ok(()) => self.message = format!("Opened homepage of {}", source.title),
                Err(error) => self.message = format!("Could not open browser: {error}"),
            }
        }
        Ok(())
    }

    fn toggle_read(&mut self) -> Result<()> {
        if let Some(article) = self.articles.get(self.selected) {
            self.store.set_read(article.id, !article.is_read)?;
            self.reload()?;
            self.message = "Read status updated".into();
        }
        Ok(())
    }

    fn mark_all_read(&mut self) -> Result<()> {
        let scope = match self.view {
            View::Sources => self.sources.get(self.selected).map(|source| source.id),
            View::Articles(id) => Some(id),
            View::Unread => None,
        };
        if matches!(self.view, View::Sources) && scope.is_none() {
            return Ok(());
        }
        let changed = self.store.mark_all_read(scope)?;
        self.reload()?;
        self.message = format!("Marked {changed} article(s) as read");
        Ok(())
    }

    fn confirm_delete(&mut self) {
        let source = match self.view {
            View::Sources => self.sources.get(self.selected),
            View::Articles(id) => self.sources.iter().find(|source| source.id == id),
            View::Unread => None,
        };
        if let Some(source) = source {
            self.dialog = Dialog::ConfirmDelete(source.id, source.title.clone());
        }
    }

    fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(4),
                Constraint::Length(5),
                Constraint::Length(2),
            ])
            .split(area);
        let total_unread: i64 = self.sources.iter().map(|source| source.unread_count).sum();
        let view_title = match self.view {
            View::Sources => "Blogs",
            View::Articles(_) => "Articles",
            View::Unread => "All unread articles",
        };
        let checking = if self.checking { "  • checking" } else { "" };
        let header = Paragraph::new(format!(
            "Blog Tracker  |  {view_title}  |  {total_unread} unread{checking}"
        ))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, parts[0]);

        let items = match self.view {
            View::Sources => self
                .sources
                .iter()
                .map(|source| {
                    let status = match source.status.as_str() {
                        "needs_config" => " · needs config",
                        "error" => " · check failed",
                        "pending" => " · pending",
                        _ => "",
                    };
                    let style = if source.unread_count > 0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!(
                        "{}  ·  {} unread{}",
                        source.title, source.unread_count, status
                    ))
                    .style(style)
                })
                .collect::<Vec<_>>(),
            _ => self
                .articles
                .iter()
                .map(|article| {
                    let marker = if article.is_read { " " } else { "●" };
                    let date = article.published_at.as_deref().unwrap_or("          ");
                    let date = date.get(..10).unwrap_or(date);
                    let prefix = if matches!(self.view, View::Unread) {
                        format!("{} · ", article.source_title)
                    } else {
                        String::new()
                    };
                    let style = if article.is_read {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(format!("{marker}  {date}  {prefix}{}", article.title))
                        .style(style)
                })
                .collect::<Vec<_>>(),
        };
        let empty = match self.view {
            View::Sources => "No blogs yet. Press a to add one.",
            View::Articles(_) => "No articles saved yet.",
            View::Unread => "No unread articles.",
        };
        if items.is_empty() {
            frame.render_widget(
                Paragraph::new(empty)
                    .block(Block::default().title(view_title).borders(Borders::ALL)),
                parts[1],
            );
        } else {
            let list = List::new(items)
                .block(Block::default().title(view_title).borders(Borders::ALL))
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("› ");
            let mut state = ListState::default();
            state.select(Some(self.selected));
            frame.render_stateful_widget(list, parts[1], &mut state);
        }

        let detail = self.detail_text();
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .block(Block::default().title("Details").borders(Borders::ALL)),
            parts[2],
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{}\n? help  ·  a add  ·  r refresh  ·  u unread  ·  q back/quit",
                self.message
            ))
            .style(Style::default().fg(Color::Gray)),
            parts[3],
        );

        self.draw_dialog(frame, area);
    }

    fn detail_text(&self) -> String {
        match self.view {
            View::Sources | View::Articles(_) => {
                let source = match self.view {
                    View::Sources => self.sources.get(self.selected),
                    View::Articles(id) => self.sources.iter().find(|source| source.id == id),
                    _ => None,
                };
                source.map_or_else(String::new, |source| {
                    let last = source.last_success_at.as_deref().unwrap_or("never");
                    let error = source.error.as_deref().unwrap_or("");
                    format!(
                        "Home: {}\nLast successful check: {}  ·  Type: {}\n{}",
                        source.home_url, last, source.kind, error
                    )
                })
            }
            View::Unread => self
                .articles
                .get(self.selected)
                .map_or_else(String::new, |article| {
                    format!(
                        "Blog: {}\nArticle URL: {}",
                        article.source_title, article.url
                    )
                }),
        }
    }

    fn draw_dialog(&self, frame: &mut Frame<'_>, outer: Rect) {
        match &self.dialog {
            Dialog::None => {}
            Dialog::Add(input) => {
                let area = centered(76, 7, outer);
                frame.render_widget(Clear, area);
                frame.render_widget(
                    Paragraph::new(format!(
                        "Paste a blog homepage or RSS/Atom URL:\n\n{input}_"
                    ))
                    .block(Block::default().title("Add blog").borders(Borders::ALL))
                    .wrap(Wrap { trim: false }),
                    area,
                );
            }
            Dialog::ConfirmDelete(_, title) => {
                let area = centered(72, 7, outer);
                frame.render_widget(Clear, area);
                frame.render_widget(
                    Paragraph::new(format!("Delete {title} and all its local article/read history?\n\nPress y to delete, n or Esc to cancel."))
                        .block(Block::default().title("Confirm deletion").borders(Borders::ALL))
                        .wrap(Wrap { trim: true }),
                    area,
                );
            }
            Dialog::Help => {
                let area = centered(76, 15, outer);
                frame.render_widget(Clear, area);
                frame.render_widget(
                    Paragraph::new("↑/↓ or j/k  move\nEnter  open blog / open selected article\nh  open blog homepage\na  add blog URL\nd  delete selected blog\nSpace  toggle article read status\nm  mark all articles in selected blog as read; in Unread, mark all read\nu  all unread  ·  s  blogs  ·  r  refresh all\nq or Esc  back / quit  ·  any key closes this help")
                        .block(Block::default().title("Help").borders(Borders::ALL))
                        .wrap(Wrap { trim: true }),
                    area,
                );
            }
        }
    }
}

fn centered(width_percent: u16, height: u16, outer: Rect) -> Rect {
    let width = outer
        .width
        .saturating_mul(width_percent)
        .saturating_div(100)
        .max(1);
    let width = width.min(outer.width);
    let height = height.min(outer.height);
    Rect {
        x: outer.x + (outer.width - width) / 2,
        y: outer.y + (outer.height - height) / 2,
        width,
        height,
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            crossterm::event::DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key)?,
                Event::Paste(text) => {
                    if let Dialog::Add(input) = &mut app.dialog {
                        input.push_str(&text);
                    }
                }
                _ => {}
            }
        }
        app.tick()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let data_dir = args
        .data_dir
        .or_else(|| dirs::data_local_dir().map(|dir| dir.join("blog-tracker-tui")))
        .context("could not determine data directory; pass --data-dir")?;
    let config_path = args
        .config
        .or_else(|| dirs::config_dir().map(|dir| dir.join("blog-tracker-tui/config.toml")))
        .context("could not determine config directory; pass --config")?;
    let db_path = data_dir.join("tracker.db");
    let mut app = App::new(db_path, config_path)?;
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    run(&mut terminal, &mut app)
}
