pub trait Replays {
    fn available(&self) -> bool;
    fn watch(&self, lap_id: &str) -> Result<(), String>;
}

pub struct NotYet;

impl Replays for NotYet {
    fn available(&self) -> bool {
        false
    }

    fn watch(&self, _lap_id: &str) -> Result<(), String> {
        Err("In-game replays are not available yet".to_owned())
    }
}
