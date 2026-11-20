#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Step {
    Full,
    TightTabs,
    SmallAccount,
    AccountIconOnly,
    LogoOnly,
    Collapsed,
}

impl Step {
    pub(super) fn tight(self) -> bool {
        self != Self::Full
    }

    pub(super) fn account(self) -> Account {
        match self {
            Self::Full | Self::TightTabs => Account::Full,
            Self::SmallAccount => Account::Small,
            _ => Account::IconOnly,
        }
    }

    pub(super) fn wordmark(self) -> bool {
        !matches!(self, Self::LogoOnly | Self::Collapsed)
    }

    pub(super) fn collapsed(self) -> bool {
        self == Self::Collapsed
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Account {
    Full,
    Small,
    IconOnly,
}

pub(super) const GAP: f32 = 16.;
pub(super) const INSET: f32 = 14.;
const LOGO: f32 = INSET + 42.;
const WORDMARK: f32 = 10. + 84.;
const ACTIONS: f32 = 44. + 4. + 44. + 6. + 10.;
const TAB_ICON_PAD: f32 = 44.;
const TAB_ICON_PAD_TIGHT: f32 = 20.;
const CHAR: f32 = 7.4;

pub(super) fn bar_width(window: f32, window_buttons: usize) -> f32 {
    window - window_buttons as f32 * f32::from(gpui_kit::component::TITLE_BAR_HEIGHT)
}

pub(super) fn tabs_width(labels: &[&str], tight: bool) -> f32 {
    let pad = if tight { TAB_ICON_PAD_TIGHT } else { TAB_ICON_PAD };
    labels
        .iter()
        .map(|l| (l.chars().count() as f32 * CHAR).max(26.) + pad)
        .sum::<f32>()
        + 2. * labels.len().saturating_sub(1) as f32
}

fn account_width(account: Account, signed_in: Option<usize>, bar: f32) -> f32 {
    match (account, signed_in) {
        (Account::Full, None) => 40. + 28. + 19. * CHAR,
        (Account::Small, None) => 40. + 28. + 6. * CHAR,
        (Account::IconOnly, None) => 40. + 18.,
        (Account::Full, Some(n)) => bar + 16. + 16. + 18. + 24. + (n as f32 * 8.).min(190.),
        (Account::Small, Some(n)) => bar + 16. + 16. + 18. + 24. + (n as f32 * 8.).min(110.),
        (Account::IconOnly, Some(_)) => bar + 8.,
    }
}

pub(super) fn choose(width: f32, labels: &[&str], signed_in: Option<usize>, bar: f32) -> Step {
    for step in [
        Step::Full,
        Step::TightTabs,
        Step::SmallAccount,
        Step::AccountIconOnly,
        Step::LogoOnly,
    ] {
        let left = LOGO + if step.wordmark() { WORDMARK } else { 0. };
        let right = ACTIONS + account_width(step.account(), signed_in, bar);
        let tabs = tabs_width(labels, step.tight());
        if width >= tabs + 2. * left.max(right) + 2. * GAP {
            return step;
        }
    }
    Step::Collapsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::tabs::Tab;

    const MIN_WINDOW: u32 = 1024;
    const WINDOWS_BUTTONS: usize = 3;
    const ACCOUNTS: [Option<usize>; 4] = [None, Some(5), Some(14), Some(32)];

    fn labels() -> Vec<&'static str> {
        Tab::ALL.iter().map(|t| t.label()).collect()
    }

    fn clearances(bar: f32, signed_in: Option<usize>) -> (Step, f32, f32) {
        let labels = labels();
        let step = choose(bar, &labels, signed_in, 68.);
        let left = LOGO + if step.wordmark() { WORDMARK } else { 0. };
        let right = ACTIONS + account_width(step.account(), signed_in, 68.);
        let tabs = tabs_width(&labels, step.tight());
        let half = bar / 2.;
        (step, half - tabs / 2. - left, bar - right - (half + tabs / 2.))
    }

    #[test]
    fn a_wide_window_keeps_everything() {
        let labels = labels();
        assert_eq!(choose(1440., &labels, None, 68.), Step::Full);
        assert_eq!(choose(1440., &labels, Some(7), 68.), Step::Full);
    }

    #[test]
    fn it_degrades_one_step_at_a_time_as_the_window_narrows() {
        let labels = labels();
        let mut last = 0;
        let order = [
            Step::Full,
            Step::TightTabs,
            Step::SmallAccount,
            Step::AccountIconOnly,
            Step::LogoOnly,
            Step::Collapsed,
        ];
        for width in (300..1500).rev().step_by(10) {
            let at = order
                .iter()
                .position(|s| *s == choose(width as f32, &labels, None, 68.))
                .unwrap();
            assert!(at >= last, "{width}");
            last = at;
        }
        assert_eq!(last, order.len() - 1);
    }

    #[test]
    fn the_tabs_never_overlap_what_is_beside_them_unless_collapsed() {
        for signed_in in ACCOUNTS {
            for width in (320..1500).step_by(5) {
                let (step, left, right) = clearances(width as f32, signed_in);
                if step.collapsed() {
                    continue;
                }
                assert!(left >= GAP - 0.01, "{width} {signed_in:?}");
                assert!(right >= GAP - 0.01, "{width} {signed_in:?}");
            }
        }
    }

    #[test]
    fn the_window_buttons_come_off_the_width_the_bar_has() {
        assert_eq!(bar_width(1024., 0), 1024.);
        assert_eq!(bar_width(1024., WINDOWS_BUTTONS), 1024. - 3. * 34.);
    }

    #[test]
    fn the_bell_clears_the_donate_tab_beside_the_window_buttons_at_every_window_width() {
        for buttons in [0, 1, WINDOWS_BUTTONS] {
            for signed_in in ACCOUNTS {
                for width in MIN_WINDOW..=2560 {
                    let bar = bar_width(width as f32, buttons);
                    let (step, left, right) = clearances(bar, signed_in);
                    assert!(!step.collapsed(), "{width} {buttons} {signed_in:?}");
                    assert!(left >= GAP - 0.01, "{width} {buttons} {signed_in:?}");
                    assert!(right >= GAP - 0.01, "{width} {buttons} {signed_in:?}");
                }
            }
        }
    }

    #[test]
    fn a_window_at_its_minimum_width_with_a_short_name_no_longer_keeps_the_wide_tabs() {
        let (step, _, right) = clearances(bar_width(MIN_WINDOW as f32, WINDOWS_BUTTONS), Some(5));
        assert_ne!(step, Step::Full);
        assert!(right >= GAP);
    }

    #[test]
    fn a_window_that_took_the_dropdown_before_now_keeps_its_tabs() {
        let labels = labels();
        assert_ne!(choose(1000., &labels, None, 68.), Step::Collapsed);
        assert!(choose(560., &labels, None, 68.) == Step::Collapsed);
    }
}
