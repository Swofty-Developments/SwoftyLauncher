//! The small Markdown subset news posts and addon descriptions are written in:
//! headings, paragraphs, lists, quotes, code, rules, pipe tables, `::: columns`
//! blocks, and inline bold, italics, code and links. HTML and images are
//! dropped, and only `https:`, `mailto:` and site-relative links are kept.

pub const SITE: &str = "https://f1.swofty.net";

pub const MAX_COLUMNS: usize = 32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub align: Vec<Align>,
    pub head: Vec<Vec<Span>>,
    pub rows: Vec<Vec<Vec<Span>>>,
}

impl Table {
    pub fn longest_words(&self) -> Vec<usize> {
        let mut longest = vec![0; self.align.len()];
        for row in std::iter::once(&self.head).chain(&self.rows) {
            for (cell, most) in row.iter().zip(&mut longest) {
                let text: String = cell.iter().map(|s| s.text.as_str()).collect();
                let word = text
                    .split_whitespace()
                    .map(|w| w.chars().map(|c| if c >= '\u{1100}' { 2 } else { 1 }).sum())
                    .max()
                    .unwrap_or(0);
                *most = (*most).max(word);
            }
        }
        longest
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(usize, Vec<Span>),
    Paragraph(Vec<Span>),
    Item {
        depth: usize,
        number: Option<u32>,
        spans: Vec<Span>,
    },
    Quote(Vec<Span>),
    Code(String),
    Rule,
    Table(Table),
    Columns(Vec<Vec<Block>>),
}

pub fn safe_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let scheme = |s: &str| url.get(..s.len()).is_some_and(|h| h.eq_ignore_ascii_case(s));
    if scheme("https://") && url.len() > 8 || scheme("mailto:") && url.len() > 7 {
        return Some(url.to_owned());
    }
    if url.starts_with('/') && !url.starts_with("//") {
        let at = url.find(['#', '?']).unwrap_or(url.len());
        let (path, rest) = url.split_at(at);
        let path = path.strip_suffix(".md").unwrap_or(path);
        return Some(format!("{SITE}{path}{rest}"));
    }
    None
}

#[derive(Default)]
enum Pending {
    #[default]
    None,
    Paragraph(String),
    Item(usize, Option<u32>, String),
    Quote(String),
}

impl Pending {
    fn append(&mut self, line: &str) -> bool {
        match self {
            Self::None => false,
            Self::Paragraph(text) | Self::Item(_, _, text) | Self::Quote(text) => {
                text.push(' ');
                text.push_str(line);
                true
            }
        }
    }

    fn take(&mut self) -> Option<Block> {
        match std::mem::take(self) {
            Self::None => None,
            Self::Paragraph(text) => Some(Block::Paragraph(inline(&text))),
            Self::Item(depth, number, text) => Some(Block::Item {
                depth,
                number,
                spans: inline(&text),
            }),
            Self::Quote(text) => Some(Block::Quote(inline(&text))),
        }
    }
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    let rest = &line[level..];
    ((1..=6).contains(&level) && (rest.is_empty() || rest.starts_with(' ')))
        .then(|| (level, rest.trim().trim_end_matches('#').trim_end()))
}

fn rule(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3 && ['-', '*', '_'].iter().any(|m| marks.chars().all(|c| c == *m))
}

fn list_item(line: &str) -> Option<(Option<u32>, &str)> {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']).filter(|r| r.starts_with(' ')) {
        return Some((None, rest.trim_start()));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let rest = line[digits..].strip_prefix(['.', ')'])?;
    if digits == 0 || digits > 9 || !rest.starts_with(' ') {
        return None;
    }
    Some((line[..digits].parse().ok(), rest.trim_start()))
}

fn html(line: &str) -> bool {
    line.starts_with('<')
        && line[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!')
}

fn fence_start(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

fn block_start(line: &str) -> bool {
    fence_start(line)
        || line.starts_with(":::")
        || line.starts_with('>')
        || html(line)
        || heading(line).is_some()
        || rule(line)
        || list_item(line).is_some()
}

fn has_pipe(line: &str) -> bool {
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '|' => return true,
            _ => {}
        }
    }
    false
}

fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('|') => cell.push('|'),
                Some(next) => {
                    cell.push('\\');
                    cell.push(next);
                }
                None => cell.push('\\'),
            },
            '|' => out.push(std::mem::take(&mut cell).trim().to_owned()),
            _ => cell.push(c),
        }
    }
    if !cell.trim().is_empty() {
        out.push(cell.trim().to_owned());
    }
    out
}

fn delimiter(line: &str) -> Option<Vec<Align>> {
    if !has_pipe(line) {
        return None;
    }
    let cells = cells(line);
    if cells.is_empty() {
        return None;
    }
    cells
        .iter()
        .map(|cell| {
            let left = cell.strip_prefix(':');
            let dashes = left.unwrap_or(cell);
            let right = dashes.strip_suffix(':');
            let dashes = right.unwrap_or(dashes);
            (!dashes.is_empty() && dashes.bytes().all(|b| b == b'-')).then_some(
                match (left.is_some(), right.is_some()) {
                    (true, true) => Align::Center,
                    (true, false) => Align::Left,
                    (false, true) => Align::Right,
                    (false, false) => Align::None,
                },
            )
        })
        .collect()
}

fn table_head(line: &str, next: Option<&str>) -> Option<Table> {
    if !has_pipe(line) {
        return None;
    }
    let align = delimiter(next?.trim())?;
    let head = cells(line);
    (head.len() == align.len() && head.len() <= MAX_COLUMNS).then(|| Table {
        align,
        head: head.iter().map(|c| inline(c)).collect(),
        rows: Vec::new(),
    })
}

fn table_row(line: &str, width: usize) -> Vec<Vec<Span>> {
    let mut row: Vec<Vec<Span>> = cells(line).iter().take(width).map(|c| inline(c)).collect();
    row.resize(width, Vec::new());
    row
}

fn loose_row(line: &str) -> Option<String> {
    let cells: Vec<String> = cells(line).into_iter().filter(|c| !c.is_empty()).collect();
    let separator = cells.iter().all(|c| c.chars().all(|ch| matches!(ch, '-' | ':')));
    (!separator).then(|| cells.join(" · "))
}

#[derive(Clone, Copy, PartialEq)]
enum Container {
    Columns,
    Column,
    Other,
}

#[derive(Default)]
struct Parser {
    blocks: Vec<Block>,
    pending: Pending,
    table: Option<Table>,
    stack: Vec<Container>,
    columns: Option<Vec<Vec<Block>>>,
    in_column: bool,
}

impl Parser {
    fn emit(&mut self, block: Block) {
        let Some(columns) = &mut self.columns else {
            self.blocks.push(block);
            return;
        };
        match columns.last_mut() {
            Some(column) if self.in_column => column.push(block),
            _ => {
                columns.push(vec![block]);
                self.in_column = true;
            }
        }
    }

    fn flush(&mut self) {
        if let Some(block) = self.pending.take() {
            self.emit(block);
        }
        if let Some(table) = self.table.take() {
            self.emit(Block::Table(table));
        }
    }

    fn container(&mut self, line: &str) {
        self.flush();
        let name = line.trim_start_matches(':').trim();
        let name = name.trim_start_matches('{').trim_start_matches('.');
        let name = name
            .split(|c: char| c.is_whitespace() || c == '}')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.is_empty() {
            match self.stack.pop() {
                Some(Container::Column) => self.in_column = false,
                Some(Container::Columns) => self.close_columns(),
                Some(Container::Other) | None => {}
            }
        } else if name == "columns" && self.columns.is_none() {
            self.columns = Some(Vec::new());
            self.in_column = false;
            self.stack.push(Container::Columns);
        } else if name == "column" && self.stack.last() == Some(&Container::Columns) {
            self.columns.get_or_insert_default().push(Vec::new());
            self.in_column = true;
            self.stack.push(Container::Column);
        } else {
            self.stack.push(Container::Other);
        }
    }

    fn close_columns(&mut self) {
        self.in_column = false;
        let mut columns: Vec<Vec<Block>> = self.columns.take().unwrap_or_default();
        columns.retain(|c| !c.is_empty());
        if columns.len() == 1 {
            self.blocks.append(&mut columns[0]);
        } else if !columns.is_empty() {
            self.blocks.push(Block::Columns(columns));
        }
    }
}

pub fn parse(text: &str) -> Vec<Block> {
    let mut lines = text.lines().peekable();
    if lines.peek().map(|l| l.trim()) == Some("---") {
        lines.next();
        lines.by_ref().find(|l| l.trim() == "---");
    }
    let mut parser = Parser::default();
    let mut fence: Option<(String, String)> = None;
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if let Some((marker, code)) = &mut fence {
            if line.starts_with(marker.as_str()) {
                let code = std::mem::take(code);
                fence = None;
                parser.emit(Block::Code(code));
            } else {
                if !code.is_empty() {
                    code.push('\n');
                }
                code.push_str(raw.trim_end());
            }
            continue;
        }
        if let Some(table) = &mut parser.table {
            if !line.is_empty() && !block_start(line) {
                let row = table_row(line, table.align.len());
                table.rows.push(row);
                continue;
            }
            parser.flush();
        }
        if fence_start(line) {
            parser.flush();
            fence = Some((line[..3].to_owned(), String::new()));
            continue;
        }
        if line.starts_with(":::") {
            parser.container(line);
            continue;
        }
        if line.is_empty() || html(line) {
            parser.flush();
            continue;
        }
        if let Some((level, title)) = heading(line) {
            parser.flush();
            parser.emit(Block::Heading(level, inline(title)));
        } else if rule(line) {
            parser.flush();
            parser.emit(Block::Rule);
        } else if let Some((number, rest)) = list_item(line) {
            parser.flush();
            let indent = raw.len() - raw.trim_start().len();
            parser.pending = Pending::Item((indent / 2).min(3), number, rest.to_owned());
        } else if let Some(rest) = line.strip_prefix('>') {
            if !matches!(parser.pending, Pending::Quote(_)) {
                parser.flush();
                parser.pending = Pending::Quote(rest.trim().to_owned());
            } else {
                parser.pending.append(rest.trim());
            }
        } else if let Some(table) = table_head(line, lines.peek().copied()) {
            parser.flush();
            lines.next();
            parser.table = Some(table);
        } else if line.starts_with('|') {
            parser.flush();
            if let Some(row) = loose_row(line) {
                parser.emit(Block::Paragraph(inline(&row)));
            }
        } else if !parser.pending.append(line) {
            parser.pending = Pending::Paragraph(line.to_owned());
        }
    }
    parser.flush();
    if let Some((_, code)) = fence {
        parser.emit(Block::Code(code));
    }
    if parser.columns.is_some() {
        parser.close_columns();
    }
    parser.blocks
}

#[derive(Clone, Default)]
struct Style {
    strong: bool,
    emphasis: bool,
    link: Option<String>,
}

fn push(out: &mut Vec<Span>, text: &str, style: &Style, code: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.strong == style.strong
        && last.emphasis == style.emphasis
        && last.code == code
        && last.link == style.link
    {
        last.text.push_str(text);
        return;
    }
    out.push(Span {
        text: text.to_owned(),
        strong: style.strong,
        emphasis: style.emphasis,
        code,
        link: style.link.clone(),
    });
}

fn closing(rest: &str, marker: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = rest[from..].find(marker).map(|i| i + from) {
        let after = rest[at + marker.len()..].chars().next();
        let before = rest[..at].chars().next_back();
        let doubled = marker.len() == 1 && (after == Some('*') || before == Some('*'));
        let inside_word = marker.starts_with('_') && after.is_some_and(char::is_alphanumeric);
        if at > 0 && !before.is_some_and(char::is_whitespace) && !doubled && !inside_word {
            return Some(at);
        }
        from = at + marker.len();
    }
    None
}

fn bracketed(rest: &str) -> Option<(&str, &str, usize)> {
    let mut depth = 0;
    let close = rest.char_indices().find_map(|(i, c)| {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    })?;
    let after = rest[close + 1..].strip_prefix('(')?;
    let end = after.find(')')?;
    let target = after[..end].split_whitespace().next().unwrap_or_default();
    let target = target
        .strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(target);
    Some((&rest[1..close], target, close + 2 + end + 1))
}

fn inline_into(text: &str, style: &Style, out: &mut Vec<Span>) {
    let mut plain = String::new();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let rest = &text[i..];
        let prev = text[..i].chars().next_back();
        match c {
            '\\' if rest[1..].starts_with(|n: char| n.is_ascii_punctuation()) => {
                plain.push_str(&rest[1..2]);
                i += 2;
                continue;
            }
            '`' => {
                let ticks = rest.bytes().take_while(|b| *b == b'`').count();
                let fence = &rest[..ticks];
                if let Some(end) = rest[ticks..].find(fence) {
                    push(out, &std::mem::take(&mut plain), style, false);
                    let code = &rest[ticks..ticks + end];
                    push(
                        out,
                        code.strip_prefix(' ')
                            .and_then(|c| c.strip_suffix(' '))
                            .unwrap_or(code),
                        style,
                        true,
                    );
                    i += 2 * ticks + end;
                    continue;
                }
            }
            '*' | '_' if c == '*' || !prev.is_some_and(char::is_alphanumeric) => {
                let marker = if rest[1..].starts_with(c) {
                    &rest[..2]
                } else {
                    &rest[..1]
                };
                let inner = &rest[marker.len()..];
                if !inner.starts_with(char::is_whitespace)
                    && let Some(end) = closing(inner, marker)
                {
                    push(out, &std::mem::take(&mut plain), style, false);
                    let mut nested = style.clone();
                    if marker.len() == 2 {
                        nested.strong = true;
                    } else {
                        nested.emphasis = true;
                    }
                    inline_into(&inner[..end], &nested, out);
                    i += 2 * marker.len() + end;
                    continue;
                }
            }
            '!' if rest.starts_with("![") => {
                if let Some((_, _, len)) = bracketed(&rest[1..]) {
                    i += 1 + len;
                    continue;
                }
            }
            '[' => {
                if let Some((label, target, len)) = bracketed(rest) {
                    push(out, &std::mem::take(&mut plain), style, false);
                    let nested = Style {
                        link: safe_url(target),
                        ..style.clone()
                    };
                    inline_into(label, &nested, out);
                    i += len;
                    continue;
                }
            }
            '<' => {
                if let Some(end) = rest.find('>') {
                    let inside = &rest[1..end];
                    if let Some(url) = safe_url(inside).filter(|_| !inside.starts_with('/')) {
                        push(out, &std::mem::take(&mut plain), style, false);
                        let nested = Style {
                            link: Some(url),
                            ..style.clone()
                        };
                        push(out, inside, &nested, false);
                        i += end + 1;
                        continue;
                    }
                    if html(rest) && !inside.contains('<') {
                        i += end + 1;
                        continue;
                    }
                }
            }
            _ => {}
        }
        plain.push(c);
        i += c.len_utf8();
    }
    push(out, &plain, style, false);
}

pub fn inline(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    inline_into(text, &Style::default(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::{Align, Block, Span, inline, parse, safe_url};

    fn texts(spans: &[Span]) -> Vec<(&str, bool, bool, Option<&str>)> {
        spans
            .iter()
            .map(|s| (s.text.as_str(), s.strong, s.code, s.link.as_deref()))
            .collect()
    }

    #[test]
    fn keeps_only_https_mailto_and_site_links() {
        assert_eq!(
            safe_url("https://f1.swofty.net/guide/").as_deref(),
            Some("https://f1.swofty.net/guide/")
        );
        assert_eq!(
            safe_url("HTTPS://discord.gg/x").as_deref(),
            Some("HTTPS://discord.gg/x")
        );
        assert_eq!(
            safe_url("mailto:support@example.com").as_deref(),
            Some("mailto:support@example.com")
        );
        assert_eq!(
            safe_url("/using/addons.md#order").as_deref(),
            Some("https://f1.swofty.net/using/addons#order")
        );
        for bad in [
            "http://f1.swofty.net",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "//evil.example",
            "steam://run/1",
            "https://",
            "relative/page",
            "https://a b",
        ] {
            assert_eq!(safe_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn reads_bold_code_and_links_inline() {
        let spans = inline(
            "**Mod Importer.** Drop a `.serps` on [the docs](/using/mods) or [x](javascript:alert(1)).",
        );
        assert_eq!(
            texts(&spans),
            [
                ("Mod Importer.", true, false, None),
                (" Drop a ", false, false, None),
                (".serps", false, true, None),
                (" on ", false, false, None),
                ("the docs", false, false, Some("https://f1.swofty.net/using/mods")),
                (" or x).", false, false, None),
            ]
        );
    }

    #[test]
    fn leaves_lone_markers_snake_case_and_html_alone() {
        let spans = inline("2 * 3 and snake_case_name <b>bold</b> ![pic](https://x/y.png) 5 * 6");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "2 * 3 and snake_case_name bold  5 * 6");
    }

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn outline(blocks: &[Block]) -> Vec<String> {
        blocks
            .iter()
            .map(|b| match b {
                Block::Heading(l, s) => format!("h{l}:{}", plain(s)),
                Block::Paragraph(s) => format!("p:{}", plain(s)),
                Block::Item { depth, number, spans } => format!("li{depth}{number:?}:{}", plain(spans)),
                Block::Quote(s) => format!("q:{}", plain(s)),
                Block::Code(c) => format!("code:{c}"),
                Block::Rule => "hr".into(),
                Block::Table(t) => {
                    let row =
                        |cells: &[Vec<Span>]| cells.iter().map(|c| plain(c)).collect::<Vec<_>>().join("|");
                    let rows: Vec<String> = std::iter::once(&t.head).chain(&t.rows).map(|r| row(r)).collect();
                    format!("table:{}", rows.join("/"))
                }
                Block::Columns(columns) => format!(
                    "columns[{}]",
                    columns
                        .iter()
                        .map(|c| outline(c).join(", "))
                        .collect::<Vec<_>>()
                        .join(" ; ")
                ),
            })
            .collect()
    }

    fn table(text: &str) -> super::Table {
        match parse(text).into_iter().next() {
            Some(Block::Table(t)) => t,
            other => panic!("no table: {other:?}"),
        }
    }

    #[test]
    fn parses_the_blocks_of_a_post() {
        let blocks = parse(
            "---\ntitle: x\n---\n\nIntro line\ncontinues.\n\n## What's new\n\n- **A.** one\n  more\n- two\n  - nested\n1. first\n\n> quoted\n\n```\nlet x = 1;\n  indented\n```\n\n---\n\n| a | b |\n|---|---|\n| c | d |\n\n::: tip\n<div>raw</div>\n:::\n",
        );
        assert_eq!(
            outline(&blocks),
            [
                "p:Intro line continues.",
                "h2:What's new",
                "li0None:A. one more",
                "li0None:two",
                "li1None:nested",
                "li0Some(1):first",
                "q:quoted",
                "code:let x = 1;\n  indented",
                "hr",
                "table:a|b/c|d",
            ]
        );
    }

    #[test]
    fn reads_the_alignment_row() {
        let t = table("| Car | Team | Year | Points |\n| :--- | :---: | ---: | --- |\n| A | B | 2013 | 12 |");
        assert_eq!(t.align, [Align::Left, Align::Center, Align::Right, Align::None]);
        let t = table("Car|Year\n:-:|-:\nA|1");
        assert_eq!(t.align, [Align::Center, Align::Right]);
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn keeps_inline_markdown_and_escaped_pipes_in_cells() {
        let t = table(
            "| Name | Notes |\n|---|---|\n| **Bold** car | `a \\| b` and [docs](/using/mods) |\n| x \\| y | back\\\\|slash |",
        );
        assert_eq!(
            texts(&t.rows[0][0]),
            [("Bold", true, false, None), (" car", false, false, None)]
        );
        assert_eq!(
            texts(&t.rows[0][1]),
            [
                ("a | b", false, true, None),
                (" and ", false, false, None),
                ("docs", false, false, Some("https://f1.swofty.net/using/mods")),
            ]
        );
        assert_eq!(plain(&t.rows[1][0]), "x | y");
        assert_eq!(plain(&t.rows[1][1]), "back\\");
    }

    #[test]
    fn pads_short_rows_and_drops_extra_cells() {
        let t = table("| a | b | c |\n|---|---|---|\n| 1 |\n| 1 | 2 | 3 | 4 |\n| | | x |");
        let rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|r| r.iter().map(|c| plain(c)).collect())
            .collect();
        assert_eq!(rows, [vec!["1", "", ""], vec!["1", "2", "3"], vec!["", "", "x"]]);
    }

    #[test]
    fn ends_a_table_at_a_blank_line_or_another_block() {
        let blocks = parse(
            "Intro\n| a | b |\n|---|---|\n| 1 | 2 |\nlazy row\n\nAfter\n\n| c |\n|---|\n| 3 |\n## Next\n| d |\n|---|\n> quote\n| e |\n|:-:|\n- item",
        );
        assert_eq!(
            outline(&blocks),
            [
                "p:Intro",
                "table:a|b/1|2/lazy row|",
                "p:After",
                "table:c/3",
                "h2:Next",
                "table:d",
                "q:quote",
                "table:e",
                "li0None:item",
            ]
        );
    }

    #[test]
    fn leaves_malformed_tables_as_text() {
        let blocks = parse(
            "| a | b |\n| c | d |\n\n| a | b |\n|---|\n\n| a | b |\n|---|-x-|\n\na | b\n---\n\n|\n\n```\n| a | b |\n|---|---|\n```",
        );
        assert_eq!(
            outline(&blocks),
            [
                "p:a · b",
                "p:c · d",
                "p:a · b",
                "p:a · b",
                "p:--- · -x-",
                "p:a | b",
                "hr",
                "code:| a | b |\n|---|---|",
            ]
        );
        let wide = |n: usize| format!("|{}\n|{}\n", "x|".repeat(n), "-|".repeat(n));
        assert!(matches!(parse(&wide(super::MAX_COLUMNS))[..], [Block::Table(_)]));
        assert!(matches!(
            parse(&wide(super::MAX_COLUMNS + 1))[..],
            [Block::Paragraph(_)]
        ));
    }

    #[test]
    fn measures_the_longest_word_of_each_column() {
        let t = table(
            "| Car | Notes | 名前 |\n|---|---|---|\n| **FW35** | a `longer` word | 日本語テキスト |\n| W04 | |",
        );
        assert_eq!(t.longest_words(), [4, 6, 14]);
    }

    #[test]
    fn keeps_html_images_and_unsafe_links_out_of_cells() {
        let t = table(
            "| <script>alert(1)</script> | <img src=\"https://evil.example/x.png\"> | ![pic](https://evil.example/y.png) | [x](javascript:alert(1)) |\n|---|---|---|---|\n| <b>bold</b> | <a href=\"https://evil.example\">a</a> | <iframe src=x> | <https://ok.example> |",
        );
        let cells: Vec<&Vec<Span>> = t.head.iter().chain(t.rows.iter().flatten()).collect();
        for cell in &cells {
            for span in cell.iter() {
                assert!(!span.text.contains('<') && !span.text.contains('>'), "{span:?}");
                assert!(span.link.is_none() || span.link.as_deref() == Some("https://ok.example"));
            }
        }
        let text: Vec<String> = cells.iter().map(|c| plain(c)).collect();
        assert_eq!(
            text,
            ["alert(1)", "", "", "x)", "bold", "a", "", "https://ok.example"]
        );
    }

    #[test]
    fn lays_blocks_out_in_columns() {
        let blocks = parse(
            "Before\n\n::: columns\n::: column\n### Setup\nRead *this*.\n:::\n::: column\n| a | b |\n|---|---|\n| 1 | 2 |\n:::\n:::\n\nAfter",
        );
        assert_eq!(
            outline(&blocks),
            [
                "p:Before",
                "columns[h3:Setup, p:Read this. ; table:a|b/1|2]",
                "p:After"
            ]
        );
    }

    #[test]
    fn reads_columns_in_pandoc_form_and_loose_content() {
        let blocks = parse(
            ":::: {.columns}\nLoose\n::: {.column width=\"40%\"}\n- one\n:::\n::: tip\nIn a tip\n:::\nTrailing\n::: column\n```\n:::\n```\n:::\n::::",
        );
        assert_eq!(
            outline(&blocks),
            ["columns[p:Loose ; li0None:one ; p:In a tip, p:Trailing ; code::::]"]
        );
    }

    #[test]
    fn keeps_single_unclosed_and_misplaced_columns_flat() {
        assert_eq!(
            outline(&parse("::: columns\n::: column\nOnly one\n:::\n:::\nNext")),
            ["p:Only one", "p:Next"]
        );
        assert_eq!(
            outline(&parse("::: column\nNot in columns\n:::\n:::\nStray close")),
            ["p:Not in columns", "p:Stray close"]
        );
        assert_eq!(
            outline(&parse(
                "::: columns\n::: column\nA\n:::\n::: column\n::: columns\nB\n:::\nC"
            )),
            ["columns[p:A ; p:B, p:C]"]
        );
    }
}
