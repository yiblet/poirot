use super::date_order::DateOrder;

/// Explicit localization policy. Month names are English; arbitrary locale/calendar
/// modes are not accepted. Fixed offsets do not model daylight-saving transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Localization {
    /// Fixed offset applied when converting the evaluation clock to local time.
    pub utc_offset_seconds: i32,
    /// Ordering of numeric components in slash-date text.
    pub date_order: DateOrder,
}
