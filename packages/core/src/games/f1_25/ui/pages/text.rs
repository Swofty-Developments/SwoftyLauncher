//! The text the pages show: cut to the game's string length, wrapped and clipped to a tile's lines, tagged
//! and cased.

/// The game's strings hold at most 255 bytes: a longer one is cut at a character and ends with "…".
pub(super) fn fit(s: String) -> String {
    fit_to(s, 255)
}

pub(super) fn fit_to(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut end = max - '…'.len_utf8();
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", s[..end].trim_end())
}

pub(super) const RESTART_NOTE: &str = "Applies after a restart.";

pub(super) fn with_note(text: &str, note: &str) -> String {
    let text = text.trim();
    if note.is_empty() {
        return fit(text.to_owned());
    }
    if text.is_empty() {
        return fit(note.to_owned());
    }
    let room = 255usize.saturating_sub(note.len() + 1);
    format!("{} {note}", fit_to(text.to_owned(), room))
}

pub(super) fn tagged(text: &str, tag: &str, style: &str) -> String {
    format!("{text}  {{s:{style}}}{tag}{{/s}}")
}

pub(super) fn wrap_text(text: &str, width: usize) -> String {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                out.push(std::mem::replace(&mut line, word.to_owned()));
            } else if line.is_empty() {
                line = word.to_owned();
            } else {
                line = format!("{line} {word}");
            }
        }
        out.push(line);
    }
    out.join("\n")
}

/// Lines a tile has for its title, details and description.
pub(super) const TILE_LINES: usize = 6;

/// Characters of a tile's title, tag included, to a line.
pub(super) const TITLE_WIDTH: usize = 30;

/// The lines a tile's title takes, as `tagged` writes it: its text and tag, the tag's markup left out.
pub(super) fn title_lines(title: &str) -> usize {
    let plain = match title.split_once("  {s:") {
        Some((text, rest)) => format!(
            "{text}  {}",
            rest.split_once('}')
                .map_or(rest, |(_, t)| t)
                .trim_end_matches("{/s}")
        ),
        None => title.to_owned(),
    };
    wrap_text(&plain, TITLE_WIDTH).lines().count().max(1)
}

/// The first `max` lines of `wrapped`, the last ending with "..." in `width` when lines were left
/// out, a word at a time.
pub(super) fn clip_lines(wrapped: &str, max: usize, width: usize) -> String {
    const MORE: &str = "...";
    let lines: Vec<&str> = wrapped.lines().collect();
    if lines.len() <= max {
        return wrapped.to_owned();
    }
    let mut kept: Vec<String> = lines[..max].iter().map(|l| (*l).to_owned()).collect();
    let last = kept.last_mut().expect("max is at least one");
    let mut words: Vec<&str> = last.split(' ').collect();
    while words.len() > 1 && words.join(" ").chars().count() + MORE.len() > width {
        words.pop();
    }
    *last = format!(
        "{}{MORE}",
        words.join(" ").trim_end_matches([',', '.', ';', ':', ' '])
    );
    kept.join("\n")
}

pub(super) fn label_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
