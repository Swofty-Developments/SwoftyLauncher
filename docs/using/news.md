---
title: News
description: The News page, the first page the launcher opens, with the Swofty Developments Discord, the latest news in full and the launcher's changelogs.
---

# News

**News** is the first page the launcher opens. It's at the top of the sidebar.

## The Discord banner

The banner at the top invites you to the Swofty Developments Discord server, where you get help from the developers and hear about new releases and addons. Select **Join the Discord**, or anywhere on the banner, to open the invite in your browser. In a short window the banner is a single row.

## News posts

The large box on the left shows every news post in full, newest first. Each post has its date, title, an optional picture and its text. Scroll inside the box to read further. Links in a post open in your browser. **Read on f1.swofty.net**, at the end of a post, opens the post on this site.

## Changelogs

The list on the right has one row per launcher release, newest first, with its version, date and summary. **Installed** marks the version you're running. Select a row to open that release's notes on this site.

When the window is too narrow for two columns, the changelogs move below the news posts.

## Where the posts come from

The launcher reads the feed from `https://f1.swofty.net/news.json` when it starts, and again when you open **News** after 30 minutes. **Refresh**, at the top right, reads it again now. The line under the **News** title says where the posts on screen came from:

| Line | Meaning |
|---|---|
| **From f1.swofty.net** | The launcher read the feed this session |
| **Checking for news…** | The launcher is reading the feed |
| **Offline · showing the news saved last time** | The launcher couldn't reach f1.swofty.net; the posts are the last feed it read and the ones this version came with |
| **Offline · showing the news that came with this version** | The launcher couldn't reach f1.swofty.net and no feed was saved |
| **Couldn't load the latest news · …** | f1.swofty.net answered without a feed; the rest of the line says which posts are shown |

The launcher keeps the last feed it read, and pictures from the newest news posts, in `cache/news` in its data folder. Each launcher release also carries its own copy of the feed. Without the feed, **News** shows those two together.

## For the site

The docs build writes `news.json` from `docs/news/*.md` (news posts) and `docs/releases/*.md` (release notes), using this front matter:

| Field | Used as |
|---|---|
| `title` | The post's title. A release's title, such as `1.3.0`, becomes "Swofty Launcher 1.3.0" |
| `description` | The summary. The changelog list shows it for each release |
| `date` | The date, as `YYYY-MM-DD`. Without it, the date the file was added to git |
| `image` | A picture shown above a news post, as an `https://` URL or a path on this site |
| `draft: true` | Leaves the file out of the feed |

A news post's text is the Markdown after its front matter and its `# ` title line. The launcher shows headings, paragraphs, bullet and numbered lists, quotes, code blocks, rules, [tables and columns](/reference/addons/publishing#markdown), bold, italics, inline code and links. It leaves out HTML, pictures inside the text and other `:::` container lines. It opens `https://` and `mailto:` links, and links that start with `/` open on this site. Other links show as plain text.

### The feed

`news.json` is `{ "posts": [...] }`, newest first. Each post has these fields:

| Field | Value |
|---|---|
| `id` | `news/<file name>` or `release/<file name>` |
| `kind` | `news` or `release` |
| `title`, `date`, `summary`, `image` | From the front matter, as above |
| `url` | The post's page on this site |
| `body` | News posts only: the post's Markdown text |

Launchers before 1.3.0 ignore `body`. A launcher that reads a feed without `body` shows the post's summary as its text.

`node docs/scripts/news.mjs` prints the feed.
