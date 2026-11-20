use super::join_classes;
use leptos::prelude::*;

#[component]
pub fn Card(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes(
        "rounded-xl border border-border bg-card text-card-foreground shadow-sm backdrop-blur-xl",
        &class,
    );
    view! { <div class=merged>{children()}</div> }
}

#[component]
pub fn CardHeader(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes("flex flex-col gap-1.5 p-4 sm:p-5", &class);
    view! { <div class=merged>{children()}</div> }
}

#[component]
pub fn CardTitle(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes("text-sm font-semibold leading-none tracking-tight", &class);
    view! { <h3 class=merged>{children()}</h3> }
}

#[component]
pub fn CardDescription(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes("text-xs text-muted-foreground", &class);
    view! { <p class=merged>{children()}</p> }
}

#[component]
pub fn CardContent(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes("p-4 pt-0 sm:p-5 sm:pt-0", &class);
    view! { <div class=merged>{children()}</div> }
}

#[component]
pub fn CardFooter(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    let merged = join_classes("flex items-center p-4 pt-0 sm:p-5 sm:pt-0", &class);
    view! { <div class=merged>{children()}</div> }
}
