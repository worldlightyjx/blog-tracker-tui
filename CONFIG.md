# Manual site configuration

The app automatically looks for a feed first, then tries to find article links in public HTML. If a blog shows **needs config**, add a rule for its article listing page. This file also lets you customize five text and selection colors. Changes are loaded when you press `r` in the TUI.

## File location

- macOS: `~/Library/Application Support/blog-tracker-tui/config.toml`
- Linux: `$XDG_CONFIG_HOME/blog-tracker-tui/config.toml`, or `~/.config/blog-tracker-tui/config.toml` when `XDG_CONFIG_HOME` is unset.
- Or pass `--config /absolute/path/config.toml` when starting the app.

Create the parent directory and file yourself if they do not exist. The SQLite database is separate; this file contains checking settings, optional per-site HTML rules, and theme colors.

For example, on macOS you can open the file with:

```sh
mkdir -p "$HOME/Library/Application Support/blog-tracker-tui"
nano "$HOME/Library/Application Support/blog-tracker-tui/config.toml"
```

On Linux:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/blog-tracker-tui"
nano "${XDG_CONFIG_HOME:-$HOME/.config}/blog-tracker-tui/config.toml"
```

## Complete example

```toml
# Defaults are 30 minutes and 3 pages if these are omitted.
check_interval_minutes = 30
max_pages = 3

[theme]
selected_fg = "#FFFFFF"
selected_bg = "#1E40AF"
unread_fg = "#FACC15"
read_fg = "#94A3B8"
accent_fg = "#67E8F9"

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

## Theme colors

The `[theme]` table is optional. Add only the fields you want to change; omitted fields keep their defaults. Use a quoted six-digit RGB hex value such as `"#FFFFFF"`. The app keeps the terminal's normal text and overall background colors.

| Field | Controls | Default |
| --- | --- | --- |
| `selected_fg` | Text on the selected row | `#FFFFFF` (white) |
| `selected_bg` | Background of the selected row | `#1E40AF` (deep blue) |
| `unread_fg` | Unread blogs and articles | `#FACC15` (gold) |
| `read_fg` | Read blogs and articles; bottom hint text | `#94A3B8` (slate gray) |
| `accent_fg` | Header/title accent | `#67E8F9` (cyan) |

Set `selected_fg` and `selected_bg` **together**. Their contrast ratio must be at least **7:1**. If either value is missing, invalid, or too low in contrast, the app uses the safe default pair and shows a theme warning in the bottom line. An invalid individual text color uses its own default. Unknown theme keys are ignored with a warning. Fix the file and press `r` to apply changes without restarting.

Here are three complete `[theme]` palettes. Copy **one** of them into your config file. You can also copy only the fields you want to override.

**Dark blue (defaults):**

```toml
[theme]
selected_fg = "#FFFFFF"
selected_bg = "#1E40AF"
unread_fg = "#FACC15"
read_fg = "#94A3B8"
accent_fg = "#67E8F9"
```

**Dark green:**

```toml
[theme]
selected_fg = "#FFFFFF"
selected_bg = "#065F46"
unread_fg = "#FDE68A"
read_fg = "#CBD5E1"
accent_fg = "#5EEAD4"
```

**Light terminal:**

```toml
[theme]
selected_fg = "#FFFFFF"
selected_bg = "#1E3A8A"
unread_fg = "#92400E"
read_fg = "#475569"
accent_fg = "#0E7490"
```

Choose colors for your actual terminal background. The app can validate the selected row's foreground/background pair; it cannot measure contrast against a terminal background it does not set.

## How to find a selector

1. Open the blog's homepage or archive page in a browser.
2. Inspect a link to a post. Choose a CSS selector that matches post links but avoids navigation and category links. For example, `article h2 a[href]`, `.post-list a.title[href]`, or `.entry` if each entry container's first link is the post.
3. Add a `[[sites]]` block with the **same URL** used in the TUI, an optional `list_url`, and the required `article_selector`.
4. If the site has a Next/Older link and the app does not follow it, add `next_page_selector` pointing to an element with an `href` attribute. Adjust `max_pages` if needed.
5. Save the file and press `r` in the TUI. The blog should change from **needs config** to a normal article count. If it remains in error, select it to read the detail message.

Only `http://` and `https://` links on the same host as the listing page are accepted as article links. A blog that loads its article list only through JavaScript cannot be handled by a CSS selector in this version.

## 中文说明

如果博客显示 **needs config**，请在配置文件中添加 `[[sites]]` 规则。macOS 默认路径为 `~/Library/Application Support/blog-tracker-tui/config.toml`；Linux 默认使用 `$XDG_CONFIG_HOME/blog-tracker-tui/config.toml`，未设置 `XDG_CONFIG_HOME` 时使用 `~/.config/blog-tracker-tui/config.toml`。也可以启动时传入 `--config /绝对路径/config.toml`。目录和文件不存在时需要自己创建。SQLite 数据库是单独的文件；这里保存检查设置、手动识别规则和颜色配置。

macOS 可先执行 `mkdir -p "$HOME/Library/Application Support/blog-tracker-tui"`，再用 `nano "$HOME/Library/Application Support/blog-tracker-tui/config.toml"` 打开配置文件。Linux 可用上方的 `XDG_CONFIG_HOME` 示例命令。保存后按 `r` 重新读取。

### 颜色配置

在文件中添加一个 `[theme]` 配置表即可更改颜色。上方完整示例和三套配色均可参考；同一文件只能有一个 `[theme]`。颜色必须写成带引号的六位 RGB 值，例如 `"#FFFFFF"`。只填写想修改的字段，其他字段沿用默认值：

| 字段 | 用途 | 默认值 |
| --- | --- | --- |
| `selected_fg` | 选中行文字 | `#FFFFFF` 白色 |
| `selected_bg` | 选中行背景 | `#1E40AF` 深蓝 |
| `unread_fg` | 未读博客与文章 | `#FACC15` 金黄 |
| `read_fg` | 已读博客与文章、底部提示 | `#94A3B8` 浅灰 |
| `accent_fg` | 顶部标题强调 | `#67E8F9` 青色 |

`selected_fg` 和 `selected_bg` 必须一起填写，并保持至少 **7:1** 的对比度。值缺失、格式错误或对比度不足时，工具会使用安全默认配色，并在底部显示提示。其他单项颜色写错时只回退该项；拼错的字段名也会提示。修改文件后在 TUI 按 `r`，无需重启即可生效。

可以选择上方的“深蓝默认”“深绿”“浅色终端”任一套示例；请只复制其中一个 `[theme]` 表。普通文字与整个界面背景继续使用终端自身的颜色，因此未读、已读和标题颜色应按你的终端背景来选。工具能够检查选中行前景与背景的对比度，无法测量终端本身的背景颜色。

### 网页识别规则

上面的 TOML 是完整示例，可以直接用于 bitsflow 的网页识别测试；该网站目前也有 Feed，因此通常不必手动配置。为其他网站配置时，按以下步骤操作：

1. 在浏览器中打开博客首页或归档页，找到一篇文章的链接并检查其 HTML 结构。
2. 选择只匹配文章链接、尽量不匹配导航和分类链接的 CSS 选择器，例如 `article h2 a[href]`、`.post-list a.title[href]`。如果每个文章容器的第一个链接就是文章，也可以选容器，例如 `.entry`。
3. 在配置文件中添加 `[[sites]]`。`url` 必须与 TUI 中添加的地址一致（末尾斜杠可以不同）；`list_url` 是可选的文章列表页；`article_selector` 必填。
4. 如果“下一页”没有被自动发现，加入指向该链接的 `next_page_selector`。`max_pages` 可设置该网站最多检查几页；文件顶部的 `max_pages` 是全局默认值。
5. 保存文件后在 TUI 按 `r` 刷新。若仍有错误，选中博客查看错误详情和上次成功检查时间。

文章链接必须是列表页同一主机上的 `http://` 或 `https://` 地址。依赖 JavaScript 才显示文章列表的网站无法通过本版 CSS 规则处理。
