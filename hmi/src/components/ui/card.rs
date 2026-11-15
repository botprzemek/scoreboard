use leptos::prelude::*;
use leptos_ui::clx;

mod components {
    use super::*;

    clx! {
        Card,
        div,
        "rounded-xl border border-border bg-card text-card-foreground shadow-sm backdrop-blur-xl"
    }
    clx! { CardHeader, div, "flex flex-col gap-1.5 p-4 sm:p-5" }
    clx! { CardTitle, h3, "text-sm font-semibold leading-none tracking-tight" }
    clx! { CardDescription, p, "text-xs text-muted-foreground" }
    clx! { CardContent, div, "p-4 pt-0 sm:p-5 sm:pt-0" }
    clx! { CardFooter, div, "flex items-center p-4 pt-0 sm:p-5 sm:pt-0" }
}

pub use components::*;
