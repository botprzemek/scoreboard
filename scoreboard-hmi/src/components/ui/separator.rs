use super::join_classes;
use leptos::prelude::*;

#[component]
pub fn Separator(#[prop(into, optional)] class: String) -> impl IntoView {
    let merged = join_classes("shrink-0 bg-border h-[1px] w-full", &class);
    view! { <div class=merged role="separator" /> }
}
