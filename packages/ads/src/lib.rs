//! Ad slots in the launcher. Each slot is a native webview over a reserved
//! rectangle of the launcher window, loading `<base>/app/<unit>`.
//!
//! On Windows the launcher hosts the webviews itself. On Linux they live in the
//! `swofty-ads` helper process, which the launcher drives with [`Command`]
//! lines on its stdin, so the launcher never links WebKitGTK.

mod protocol;
#[cfg(feature = "view")]
pub mod view;

pub use protocol::Command;

/// The page for one ad unit, with `background` behind it while the ad loads.
pub fn slot_url(base: &str, unit: &str, background: [u8; 3]) -> String {
    let [r, g, b] = background;
    format!(
        "{}/app/{unit}?bg={r:02x}{g:02x}{b:02x}",
        base.trim_end_matches('/')
    )
}

/// The background a [`slot_url`] asks for.
pub fn slot_background(url: &str) -> Option<[u8; 3]> {
    let hex = url.split_once("bg=")?.1.get(..6)?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Whether `url` is on the same origin as `home`, so the slot may navigate to it.
pub fn same_origin(home: &str, url: &str) -> bool {
    origin(home).is_some_and(|o| origin(url) == Some(o))
}

/// Whether the launcher may hand `url` to the default browser.
pub fn openable(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 4096
        && !url.contains(char::is_whitespace)
}

fn origin(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let scheme = url.len() - rest.len();
    Some(&url[..scheme + end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_url_names_the_unit_and_background() {
        assert_eq!(
            slot_url("https://ads.swofty.net/", "right-rail-1", [0x1a, 0x1b, 0x0c]),
            "https://ads.swofty.net/app/right-rail-1?bg=1a1b0c"
        );
    }

    #[test]
    fn a_slot_url_gives_its_background_back() {
        let url = slot_url("https://ads.swofty.net", "video", [0x0b, 0xfe, 0x40]);
        assert_eq!(slot_background(&url), Some([0x0b, 0xfe, 0x40]));
        assert_eq!(slot_background("https://ads.swofty.net/app/video"), None);
        assert_eq!(
            slot_background("https://ads.swofty.net/app/video?bg=zz0000"),
            None
        );
    }

    #[test]
    fn only_the_home_origin_counts_as_same_origin() {
        let home = "https://ads.swofty.net/app/video?bg=000000";
        assert!(same_origin(home, "https://ads.swofty.net/app/video"));
        assert!(same_origin(home, "https://ads.swofty.net"));
        assert!(!same_origin(
            home,
            "https://ads.swofty.net.evil.example/app/video"
        ));
        assert!(!same_origin(home, "http://ads.swofty.net/app/video"));
        assert!(!same_origin(
            home,
            "https://example.com/?u=https://ads.swofty.net"
        ));
        assert!(!same_origin(home, "about:blank"));
    }

    #[test]
    fn only_web_links_open_in_the_browser() {
        assert!(openable("https://example.com/landing?id=1"));
        assert!(openable("http://example.com"));
        assert!(!openable("file:///etc/passwd"));
        assert!(!openable("javascript:alert(1)"));
        assert!(!openable("steam://run/1"));
        assert!(!openable("https://example.com/a b"));
    }
}
