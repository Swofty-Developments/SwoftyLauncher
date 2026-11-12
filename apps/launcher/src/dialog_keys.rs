use std::rc::Rc;

use gpui_kit::*;

const DIALOG: &str = "Dialog";

const ARROWS: [(&str, &str); 4] = [
    ("left", "root::TabPrev"),
    ("up", "root::TabPrev"),
    ("right", "root::Tab"),
    ("down", "root::Tab"),
];

pub(crate) fn init(cx: &mut App) {
    bind_arrows(cx);
}

fn bind_arrows(cx: &mut App) {
    let Ok(context) = KeyBindingContextPredicate::parse(DIALOG).map(Rc::new) else {
        return;
    };
    let mut bindings = Vec::new();
    for (key, action) in ARROWS {
        match cx
            .build_action(action, None)
            .map_err(|e| e.to_string())
            .and_then(|a| {
                KeyBinding::load(key, a, Some(context.clone()), false, None, cx).map_err(|e| e.to_string())
            }) {
            Ok(binding) => bindings.push(binding),
            Err(e) => crate::app_log::note(&format!("ui: no {key} key in dialogs: {e}")),
        }
    }
    cx.bind_keys(bindings);
}
