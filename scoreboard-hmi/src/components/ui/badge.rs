use super::join_classes;
use leptos::prelude::*;

const BADGE_BASE: &str = "inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors select-none whitespace-nowrap";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Success,
    Warning,
    Destructive,
}

impl BadgeVariant {
    const fn class(self) -> &'static str {
        match self {
            Self::Default => "border-transparent bg-primary/15 text-primary",
            Self::Secondary => "border-transparent bg-secondary text-secondary-foreground",
            Self::Outline => "border-border text-foreground bg-transparent",
            Self::Success => {
                "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
            }
            Self::Warning => {
                "border-transparent bg-amber-500/15 text-amber-600 dark:text-amber-400"
            }
            Self::Destructive => "border-transparent bg-destructive/15 text-destructive",
        }
    }
}

#[component]
pub fn Badge(
    #[prop(into, optional)] variant: Signal<BadgeVariant>,
    #[prop(into, optional)] class: String,
    children: Children,
) -> impl IntoView {
    let classes = Memo::new(move |_| {
        let base = format!("{BADGE_BASE} {}", variant.get().class());
        join_classes(&base, &class)
    });

    view! {
        <span class=move || classes.get()>{children()}</span>
    }
}
