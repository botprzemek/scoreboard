use leptos::prelude::*;
use tw_merge::*;

#[derive(TwClass)]
#[tw(
    class = "inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors select-none whitespace-nowrap"
)]
pub struct BadgeClass {
    pub variant: BadgeVariant,
}

#[derive(TwVariant)]
pub enum BadgeVariant {
    #[tw(default, class = "border-transparent bg-primary/15 text-primary")]
    Default,
    #[tw(class = "border-transparent bg-secondary text-secondary-foreground")]
    Secondary,
    #[tw(class = "border-border text-foreground bg-transparent")]
    Outline,
    #[tw(class = "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400")]
    Success,
    #[tw(class = "border-transparent bg-amber-500/15 text-amber-600 dark:text-amber-400")]
    Warning,
    #[tw(class = "border-transparent bg-destructive/15 text-destructive")]
    Destructive,
}

#[component]
pub fn Badge(
    #[prop(into, optional)] variant: Signal<BadgeVariant>,
    #[prop(into, optional)] class: String,
    children: Children,
) -> impl IntoView {
    let classes = Memo::new(move |_| {
        let base = BadgeClass {
            variant: variant.get(),
        }
        .to_class();
        tw_merge!(&base, &class)
    });

    view! {
        <span class=move || classes.get()>{children()}</span>
    }
}
