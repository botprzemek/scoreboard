use leptos::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn Separator(#[prop(into, optional)] class: String) -> impl IntoView {
    let merged = tw_merge!("shrink-0 bg-border h-[1px] w-full", &class);
    view! { <div class=merged role="separator" /> }
}
