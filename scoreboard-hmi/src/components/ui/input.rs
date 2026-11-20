use super::join_classes;
use leptos::prelude::*;

#[component]
pub fn Input(
    #[prop(into, optional)] value: Signal<String>,
    #[prop(into, optional)] placeholder: String,
    #[prop(into, optional)] class: String,
    #[prop(optional, default = "text")] r#type: &'static str,
    #[prop(optional)] on_input: Option<Callback<String>>,
) -> impl IntoView {
    let merged = join_classes(
        "flex h-9 w-full rounded-lg border border-input bg-background/80 px-3 py-1.5 text-xs text-foreground shadow-2xs transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        &class,
    );

    view! {
        <input
            type=r#type
            class=merged
            placeholder=placeholder
            prop:value=move || value.get()
            on:input=move |ev| {
                if let Some(cb) = on_input {
                    cb.run(event_target_value(&ev));
                }
            }
        />
    }
}
