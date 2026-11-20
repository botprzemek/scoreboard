use super::join_classes;
use leptos::prelude::*;

const BUTTON_BASE: &str = "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-xs font-medium transition-all cursor-pointer select-none active:scale-[0.98] disabled:pointer-events-none disabled:opacity-45 focus-visible:ring-2 focus-visible:ring-ring";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Success,
}

impl ButtonVariant {
    const fn class(self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground shadow-sm hover:opacity-90",
            Self::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80",
            Self::Outline => {
                "border border-border bg-card hover:bg-secondary/70 text-foreground shadow-2xs"
            }
            Self::Ghost => "hover:bg-secondary text-foreground",
            Self::Destructive => {
                "bg-destructive text-destructive-foreground shadow-sm hover:opacity-90"
            }
            Self::Success => "bg-emerald-500 text-white shadow-sm hover:bg-emerald-600",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    #[default]
    Default,
    Sm,
    Lg,
    Icon,
}

impl ButtonSize {
    const fn class(self) -> &'static str {
        match self {
            Self::Default => "h-8 px-3 py-1.5",
            Self::Sm => "h-7 rounded-md px-2.5 text-[11px]",
            Self::Lg => "h-10 rounded-xl px-4 text-sm",
            Self::Icon => "h-8 w-8 p-0",
        }
    }
}

#[component]
pub fn Button(
    #[prop(into, optional)] variant: Signal<ButtonVariant>,
    #[prop(into, optional)] size: Signal<ButtonSize>,
    #[prop(into, optional)] class: String,
    #[prop(into, optional)] disabled: Signal<bool>,
    #[prop(optional)] title: Option<&'static str>,
    #[prop(optional, default = "button")] r#type: &'static str,
    children: Children,
) -> impl IntoView {
    let classes = Memo::new(move |_| {
        let base = format!(
            "{BUTTON_BASE} {} {}",
            variant.get().class(),
            size.get().class()
        );
        join_classes(&base, &class)
    });

    view! {
        <button
            type=r#type
            title=title
            disabled=move || disabled.get()
            class=move || classes.get()
        >
            {children()}
        </button>
    }
}
