#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Flags {
    pub record_mouse_state: bool,
    pub split: bool,
}

pub const FLAGS: Flags = Flags {
    // record_mouse_state: cfg!(debug_assertions),
    record_mouse_state: true,
    split: false,
};
