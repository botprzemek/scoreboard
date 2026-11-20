pub mod badge;
pub mod button;
pub mod card;
pub mod input;
pub mod separator;

pub(super) fn join_classes(base: &str, extra: &str) -> String {
    if extra.is_empty() {
        base.to_string()
    } else {
        format!("{base} {extra}")
    }
}
