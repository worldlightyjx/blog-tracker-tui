# Manual site configuration

The app automatically looks for a feed first, then tries to find article links in public HTML. If a blog shows **needs config**, add a rule for its article listing page. Rules are loaded on the next refresh.

## File location

- macOS: `~/Library/Application Support/blog-tracker-tui/config.toml`
- Linux: `$XDG_CONFIG_HOME/blog-tracker-tui/config.toml`, or `~/.config/blog-tracker-tui/config.toml` when `XDG_CONFIG_HOME` is unset.
- Or pass `--config /absolute/path/config.toml` when starting the app.

Create the parent directory and file yourself if they do not exist. The SQLite database is separate; this file only contains checking settings and optional per-site HTML rules.

## Complete example

```toml
# Defaults are 30 minutes and 3 pages if these are omitted.
check_interval_minutes = 30
max_pages = 3

[[sites]]
# Must match the URL you entered in the TUI (a trailing slash is ignored).
url = "https://bitsflow.org/"
# Optional: use an archive page when the homepage is not the article list.
list_url = "https://bitsflow.org/"
# Match article links, or containers whose first link is the article link.
article_selector = "article a[href]"
# Optional: use this if the site's Next link is not recognized automatically.
# next_page_selector = "a[rel='next']"
max_pages = 5
```

`bitsflow.org` currently has a feed, so this example deliberately forces HTML checking to demonstrate a working manual rule. For a different site, replace the URLs and CSS selector with that site's article list.

## How to find a selector

1. Open the blog's homepage or archive page in a browser.
2. Inspect a link to a post. Choose a CSS selector that matches post links but avoids navigation and category links. For example, `article h2 a[href]`, `.post-list a.title[href]`, or `.entry` if each entry container's first link is the post.
3. Add a `[[sites]]` block with the **same URL** used in the TUI, an optional `list_url`, and the required `article_selector`.
4. If the site has a Next/Older link and the app does not follow it, add `next_page_selector` pointing to an element with an `href` attribute. Adjust `max_pages` if needed.
5. Save the file and press `r` in the TUI. The blog should change from **needs config** to a normal article count. If it remains in error, select it to read the detail message.

Only `http://` and `https://` links on the same host as the listing page are accepted as article links. A blog that loads its article list only through JavaScript cannot be handled by a CSS selector in this version.

## 中文说明

如果博客显示 **needs config**，请在配置文件中添加 `[[sites]]` 规则。macOS 默认路径为 `~/Library/Application Support/blog-tracker-tui/config.toml`；Linux 默认使用 `$XDG_CONFIG_HOME/blog-tracker-tui/config.toml`，未设置 `XDG_CONFIG_HOME` 时使用 `~/.config/blog-tracker-tui/config.toml`。也可以启动时传入 `--config /绝对路径/config.toml`。目录和文件不存在时需要自己创建。SQLite 数据库是单独的文件；这里仅保存检查设置和手动识别规则。

上面的 TOML 是完整示例，可以直接用于 bitsflow 的网页识别测试；该网站目前也有 Feed，因此通常不必手动配置。为其他网站配置时，按以下步骤操作：

1. 在浏览器中打开博客首页或归档页，找到一篇文章的链接并检查其 HTML 结构。
2. 选择只匹配文章链接、尽量不匹配导航和分类链接的 CSS 选择器，例如 `article h2 a[href]`、`.post-list a.title[href]`。如果每个文章容器的第一个链接就是文章，也可以选容器，例如 `.entry`。
3. 在配置文件中添加 `[[sites]]`。`url` 必须与 TUI 中添加的地址一致（末尾斜杠可以不同）；`list_url` 是可选的文章列表页；`article_selector` 必填。
4. 如果“下一页”没有被自动发现，加入指向该链接的 `next_page_selector`。`max_pages` 可设置该网站最多检查几页；文件顶部的 `max_pages` 是全局默认值。
5. 保存文件后在 TUI 按 `r` 刷新。若仍有错误，选中博客查看错误详情和上次成功检查时间。

文章链接必须是列表页同一主机上的 `http://` 或 `https://` 地址。依赖 JavaScript 才显示文章列表的网站无法通过本版 CSS 规则处理。
