use gpui_kit::*;

/// A state the shell holds that starts background work of its own. It names what it borrows from
/// the shell beside itself, and what it reports back when other features have to follow.
pub(crate) trait Hosted: Sized + 'static {
    type Needs<'a>;
    type Change;
}

/// The shell as one of its states sees it. A state's methods that start background work take
/// `&mut impl Host<Self>`, so they reach only the state and its [`Hosted::Needs`], and the work
/// writes its result back through the same view once it finishes. `changed` is where the shell
/// routes what the state reports to the other features.
pub(crate) trait Host<T: Hosted>: Sized + 'static {
    fn split(&mut self) -> (&mut T, T::Needs<'_>);
    fn changed(&mut self, change: T::Change, cx: &mut Context<Self>);
}

/// A hosted state whose work stops on prompts in the window: a dialog to answer, a notice to read.
pub(crate) trait Prompted: Hosted {
    type Prompt;
}

/// A [`Host`] that also opens a state's prompts in the window.
pub(crate) trait PromptHost<T: Prompted>: Host<T> {
    fn prompt(&mut self, prompt: T::Prompt, window: &mut Window, cx: &mut Context<Self>);
}

/// The `changed` of a state that reports nothing back, whose `Change` is `Infallible`.
macro_rules! reports_nothing {
    () => {
        fn changed(&mut self, change: std::convert::Infallible, _: &mut gpui_kit::Context<Self>) {
            match change {}
        }
    };
}

pub(crate) use reports_nothing;
