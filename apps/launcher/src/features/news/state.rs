use super::feed::{CACHE_DIR, FEED_FILE, Fetched, Post, STALE, Trouble, embedded, fetch, merge, parse};
use super::markdown::{Block, safe_url};
use super::view::parsed_body;
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted, reports_nothing};
use crate::ui::scale::px;
use gpui_kit::*;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use swofty_i18n::{tr, trf};

pub(super) const STACK_BELOW: f32 = 680.;

pub struct News {
    pub posts: Vec<Post>,
    pub(super) images: HashMap<String, Arc<Image>>,
    pub(super) blocks: Vec<Vec<Block>>,
    pub(super) revision: u64,
    pub(super) checked: Option<Instant>,
    pub(super) live: bool,
    pub(super) saved: bool,
    pub(super) fetching: bool,
    pub(super) trouble: Option<Trouble>,
}

#[derive(Clone, Copy)]
pub(super) enum NewsRow {
    Header(usize, bool),
    Block(usize, usize),
    Link(usize),
    Separator,
}

#[derive(Clone, Copy)]
pub(super) enum StackedRow {
    Banner,
    ArticlesTop,
    Article(usize),
    ArticlesBottom,
    ReleasesHeader,
    Release(usize),
    ReleasesBottom,
}

#[derive(Clone, Copy)]
pub(super) enum NewsPane {
    Articles,
    Releases,
    Stacked,
}

pub(crate) struct NewsScroll {
    pub(super) articles: ListState,
    pub(super) releases: ListState,
    pub(super) stacked: ListState,
    pub(super) rows: Vec<NewsRow>,
    pub(super) stacked_rows: Vec<StackedRow>,
    pub(super) release_indices: Vec<usize>,
    pub(super) revision: Option<u64>,
    pub(super) short: bool,
    pub(super) stacked_layout: bool,
    pub(super) show_banner: bool,
    pub(super) motion: [crate::ui::scroll::SmoothScroll; 3],
}

impl Default for NewsScroll {
    fn default() -> Self {
        Self {
            articles: ListState::new(0, ListAlignment::Top, px(200.)),
            releases: ListState::new(0, ListAlignment::Top, px(100.)),
            stacked: ListState::new(0, ListAlignment::Top, px(200.)),
            rows: Vec::new(),
            stacked_rows: Vec::new(),
            release_indices: Vec::new(),
            revision: None,
            short: false,
            stacked_layout: false,
            show_banner: true,
            motion: Default::default(),
        }
    }
}

impl NewsScroll {
    pub(crate) fn cancel(&self) {
        for motion in &self.motion {
            motion.cancel();
        }
    }

    pub(super) fn list(&self, pane: NewsPane) -> &ListState {
        match pane {
            NewsPane::Articles => &self.articles,
            NewsPane::Releases => &self.releases,
            NewsPane::Stacked => &self.stacked,
        }
    }

    pub(super) fn sync(&mut self, news: &News, short: bool, stacked: bool, show_banner: bool) {
        if self.stacked_layout != stacked {
            self.stacked_layout = stacked;
            self.cancel();
        }
        if self.show_banner != show_banner {
            self.show_banner = show_banner;
            if self.revision.is_some() {
                if show_banner {
                    self.stacked_rows.insert(0, StackedRow::Banner);
                    self.stacked.splice(0..0, 1);
                } else if matches!(self.stacked_rows.first(), Some(StackedRow::Banner)) {
                    self.stacked_rows.remove(0);
                    self.stacked.splice(0..1, 0);
                }
            }
        }
        if self.short != short {
            self.short = short;
            if self.show_banner && self.stacked.item_count() > 0 {
                self.stacked.remeasure_items(0..1);
            }
        }
        if self.revision == Some(news.revision) {
            return;
        }
        self.cancel();
        let first = self.revision.is_none();
        self.revision = Some(news.revision);
        self.rows.clear();
        self.release_indices.clear();
        for (i, post) in news.posts.iter().enumerate() {
            if post.release() {
                self.release_indices.push(i);
                continue;
            }
            let first = self.rows.is_empty();
            if !first {
                self.rows.push(NewsRow::Separator);
            }
            self.rows.push(NewsRow::Header(i, first));
            self.rows
                .extend((0..news.blocks[i].len()).map(|b| NewsRow::Block(i, b)));
            if post.url.as_deref().and_then(safe_url).is_some() {
                self.rows.push(NewsRow::Link(i));
            }
        }
        self.stacked_rows.clear();
        if show_banner {
            self.stacked_rows.push(StackedRow::Banner);
        }
        self.stacked_rows.push(StackedRow::ArticlesTop);
        self.stacked_rows
            .extend((0..self.rows.len()).map(StackedRow::Article));
        self.stacked_rows
            .extend([StackedRow::ArticlesBottom, StackedRow::ReleasesHeader]);
        self.stacked_rows
            .extend((0..self.release_indices.len()).map(StackedRow::Release));
        self.stacked_rows.push(StackedRow::ReleasesBottom);
        // Preserve the current item and its offset while remeasuring refreshed content.
        for (state, count) in [
            (&self.articles, self.rows.len()),
            (&self.releases, self.release_indices.len()),
            (&self.stacked, self.stacked_rows.len()),
        ] {
            if first {
                state.reset_with_uniform_height(count, px(64.));
                continue;
            }
            let old = state.item_count();
            if count > old {
                state.splice(old..old, count - old);
            } else if count < old {
                state.splice(count..old, 0);
            }
            state.remeasure_items(0..count);
        }
    }
}

impl News {
    pub fn load(root: &Path) -> Self {
        let cached = std::fs::read_to_string(root.join(CACHE_DIR).join(FEED_FILE))
            .ok()
            .and_then(|t| parse(&t))
            .unwrap_or_default();
        let saved = !cached.is_empty();
        let posts = merge(embedded(), cached);
        let blocks = posts.iter().map(parsed_body).collect();
        Self {
            saved,
            posts,
            blocks,
            revision: 0,
            images: HashMap::new(),
            checked: None,
            live: false,
            fetching: false,
            trouble: None,
        }
    }

    pub(super) fn stale(&self) -> bool {
        !self.fetching && self.checked.is_none_or(|t| t.elapsed() >= STALE)
    }

    pub(super) fn subtitle(&self) -> String {
        if self.fetching {
            return tr("Checking for news…").into();
        }
        if self.live {
            return String::new();
        }
        let shown = if self.saved {
            tr("showing the news saved last time")
        } else {
            tr("showing the news that came with this version")
        };
        match self.trouble {
            Some(Trouble::Failed) => trf!("Couldn't load the latest news · {shown}", shown = shown),
            _ => trf!("Offline · {shown}", shown = shown),
        }
    }
}

pub(crate) struct NewsNeeds<'a> {
    pub(crate) root: &'a Path,
}

impl Hosted for News {
    type Needs<'a> = NewsNeeds<'a>;
    type Change = Infallible;
}

impl Host<News> for Shell {
    fn split(&mut self) -> (&mut News, NewsNeeds<'_>) {
        (
            &mut self.news,
            NewsNeeds {
                root: &self.store.root,
            },
        )
    }

    reports_nothing!();
}

impl News {
    pub(crate) fn fetch<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (news, needs) = host.split();
        if news.fetching {
            return;
        }
        news.fetching = true;
        let root = needs.root.to_path_buf();
        let shown = news.posts.clone();
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move { fetch(&root, shown) })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.took_fetch(fetched);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn took_fetch(&mut self, fetched: Fetched) {
        self.fetching = false;
        self.checked = Some(Instant::now());
        self.trouble = fetched.error.as_ref().map(|(t, _)| *t);
        if let Some((_, e)) = &fetched.error {
            eprintln!("swofty-launcher: could not check for news: {e}");
        }
        let mut changed = !fetched.images.is_empty();
        if let Some(posts) = fetched.posts {
            if self.posts != posts {
                changed = true;
                self.blocks = posts.iter().map(parsed_body).collect();
                self.posts = posts;
            }
            self.live = true;
        }
        self.images.extend(fetched.images);
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }
}
