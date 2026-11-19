use leptos::prelude::*;
use tw_merge::*;

#[derive(TwClass)]
#[tw(
    class = "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-xs font-medium transition-all cursor-pointer select-none active:scale-[0.98] disabled:pointer-events-none disabled:opacity-45 focus-visible:ring-2 focus-visible:ring-ring"
)]
pub struct ButtonClass {
    pub variant: ButtonVariant,
    pub size: ButtonSize,
}

#[derive(TwVariant)]
pub enum ButtonVariant {
    #[tw(default, class = "bg-primary text-primary-foreground shadow-sm hover:opacity-90")]
    Default,
    #[tw(class = "bg-secondary text-secondary-foreground hover:bg-secondary/80")]
    Secondary,
    #[tw(class = "border border-border bg-card hover:bg-secondary/70 text-foreground shadow-2xs")]
    Outline,
    #[tw(class = "hover:bg-secondary text-foreground")]
    Ghost,
    #[tw(class = "bg-destructive text-destructive-foreground shadow-sm hover:opacity-90")]
    Destructive,
    #[tw(class = "bg-emerald-500 text-white shadow-sm hover:bg-emerald-600")]
    Success,
}

#[derive(TwVariant)]
pub enum ButtonSize {
    #[tw(default, class = "h-8 px-3 py-1.5")]
    Default,
    #[tw(class = "h-7 rounded-md px-2.5 text-[11px]")]
    Sm,
    #[tw(class = "h-10 rounded-xl px-4 text-sm")]
    Lg,
    #[tw(class = "h-8 w-8 p-0")]
    Icon,
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
        let base = ButtonClass {
            variant: variant.get(),
            size: size.get(),
        }
        .to_class();
        tw_merge!(&base, &class)
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
