# Blog Tracker TUI

A local terminal app that tracks new posts from RSS/Atom/JSON feeds and public blog pages. It stores article titles and URLs, then opens posts in your default browser. It does not render article content in the terminal.

## Install and run

Requires a current Rust toolchain on macOS or Linux.

```sh
cargo install --path .
blog-tracker-tui
```

Install directly from the public GitHub repository:

```sh
cargo install --git https://github.com/worldlightyjx/blog-tracker-tui.git
```

The planned crates.io release will follow after the first version stabilizes.

## Use

1. Press `a` and paste a blog homepage or feed URL.
2. The app checks in the background. It prefers a discovered feed and falls back to article links on public HTML pages.
3. Select a blog with `Enter`, then select an article with `Enter` to open it in your default browser. A successfully opened article is marked read.
4. Use `u` for all unread articles or `r` to check all blogs now.

| Key | Action |
| --- | --- |
| `↑`/`↓`, `j`/`k` | Move selection |
| `Enter` | Open blog list or selected article |
| `h` | Open selected blog homepage |
| `a` | Add a blog or feed URL |
| `d` | Delete a blog and its local history, after confirmation |
| `Space` | Toggle an article's read status |
| `m` | Mark all articles in the current blog as read; in Unread, mark all read |
| `u` / `s` | Show all unread articles / blogs |
| `r` | Refresh all blogs |
| `q` / `Esc` | Back or quit |
| `?` | Help |

On the first successful check, the three newest existing articles are unread. Older articles are stored as read, so they are not mistaken for new posts later. If fewer than three exist, all are unread. Publication dates determine recency when available; otherwise the page or feed order does.

The app checks at startup without delaying the interface, then every 30 minutes while running. A failed check retains saved articles and shows the error and last successful check time. Sites without recognizable article links remain saved with a **needs config** status.

## Configuration and data

Manual rules for web-only blogs live in a TOML config file. See [CONFIG.md](CONFIG.md) for paths, CSS selector examples, pagination, and troubleshooting. Changes are read on the next refresh; press `r` after editing.

By default, the app uses the platform's local data and config directories under `blog-tracker-tui/`. You can choose explicit paths:

```sh
blog-tracker-tui --data-dir /path/to/data --config /path/to/config.toml
```

The SQLite database in the data directory stores subscriptions, article links, and read status. Deleting a blog from the TUI removes its saved articles and read history. The config file only holds optional manual recognition rules.

## Scope and limitations

- Public sites only; login-required blogs are outside the first version.
- Pages that require JavaScript to display their article list are not supported yet. See [TODO.md](TODO.md).
- HTML recognition is best effort. A manual CSS rule can adapt to a site's layout.
- Pagination is bounded. Posts no longer present in a feed or reachable list page cannot be recovered after a long gap.
- Existing article URLs are never marked new again when only their title or content changes.

The four initial acceptance examples are [bitsflow](https://bitsflow.org/), [tw93](https://tw93.fun/), [Kawabangga](https://www.kawabangga.com/), and [37signals Engineering](https://dev.37signals.com/). All currently expose usable feeds; bitsflow also works with the manual HTML example in [CONFIG.md](CONFIG.md).

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

The optional live-site acceptance checks require network access:

```sh
cargo test --test live_sites -- --ignored --nocapture
```

## 中文快速开始

本工具在终端中记录博客新文章，并用系统默认浏览器打开文章。首次检查最多把最近三篇标为未读；之后仅把新 URL 视为新文章。启动后立即显示本地记录，后台检查，每 30 分钟自动检查一次，也可按 `r` 手动刷新。

在项目目录执行 `cargo install --path .`，然后运行 `blog-tracker-tui`。按 `a` 添加博客主页或 Feed 地址，`Enter` 进入博客或打开文章，`u` 查看全部未读，`Space` 切换已读状态，`m` 全部标为已读，`h` 打开博客首页，`d` 删除博客及本地历史。无 RSS 网站的手动配置方法见 [CONFIG.md](CONFIG.md) 的中文部分。

代码采用 MIT 许可证。首版计划通过公开 GitHub 仓库供 `cargo install --git` 安装，稳定后发布到 crates.io。
