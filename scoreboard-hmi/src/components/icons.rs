use leptos::prelude::*;

#[component]
fn LucideIcon(#[prop(into, optional)] class: String, children: Children) -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class=class
        >
            {children()}
        </svg>
    }
}

#[component]
pub fn Cpu(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <rect width="16" height="16" x="4" y="4" rx="2" />
            <rect width="6" height="6" x="9" y="9" rx="1" />
            <path d="M15 2v2M15 20v2M2 15h2M2 9h2M20 15h2M20 9h2M9 2v2M9 20v2" />
        </LucideIcon>
    }
}

#[component]
pub fn Eye(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" />
            <circle cx="12" cy="12" r="3" />
        </LucideIcon>
    }
}

#[component]
pub fn Menu(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M4 12h16M4 6h16M4 18h16" />
        </LucideIcon>
    }
}

#[component]
pub fn Monitor(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <rect width="20" height="14" x="2" y="3" rx="2" />
            <line x1="8" x2="16" y1="21" y2="21" />
            <line x1="12" x2="12" y1="17" y2="21" />
        </LucideIcon>
    }
}

#[component]
pub fn Pause(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <rect x="14" y="4" width="4" height="16" rx="1" />
            <rect x="6" y="4" width="4" height="16" rx="1" />
        </LucideIcon>
    }
}

#[component]
pub fn Pencil(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
            <path d="m15 5 4 4" />
        </LucideIcon>
    }
}

#[component]
pub fn Play(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <polygon points="6 3 20 12 6 21 6 3" />
        </LucideIcon>
    }
}

#[component]
pub fn Plus(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M5 12h14M12 5v14" />
        </LucideIcon>
    }
}

#[component]
pub fn RefreshCw(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
            <path d="M21 3v5h-5" />
            <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
            <path d="M8 16H3v5" />
        </LucideIcon>
    }
}

#[component]
pub fn RotateCcw(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
        </LucideIcon>
    }
}

#[component]
pub fn Server(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <rect width="20" height="8" x="2" y="2" rx="2" ry="2" />
            <rect width="20" height="8" x="2" y="14" rx="2" ry="2" />
            <line x1="6" x2="6.01" y1="6" y2="6" />
            <line x1="6" x2="6.01" y1="18" y2="18" />
        </LucideIcon>
    }
}

#[component]
pub fn Square(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <rect width="18" height="18" x="3" y="3" rx="2" />
        </LucideIcon>
    }
}

#[component]
pub fn Terminal(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <polyline points="4 17 10 11 4 5" />
            <line x1="12" x2="20" y1="19" y2="19" />
        </LucideIcon>
    }
}

#[component]
pub fn Trash2(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M3 6h18" />
            <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
            <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
            <line x1="10" x2="10" y1="11" y2="17" />
            <line x1="14" x2="14" y1="11" y2="17" />
        </LucideIcon>
    }
}

#[component]
pub fn Trophy(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6" />
            <path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18" />
            <path d="M4 22h16" />
            <path d="M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22" />
            <path d="M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22" />
            <path d="M18 2H6v7a6 6 0 0 0 12 0V2Z" />
        </LucideIcon>
    }
}

#[component]
pub fn Wifi(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M12 20h.01" />
            <path d="M2 8.82a15 15 0 0 1 20 0" />
            <path d="M5 12.859a10 10 0 0 1 14 0" />
            <path d="M8.5 16.429a5 5 0 0 1 7 0" />
        </LucideIcon>
    }
}

#[component]
pub fn WifiOff(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M12 20h.01" />
            <path d="M8.5 16.429a5 5 0 0 1 7 0" />
            <path d="M5 12.859a10 10 0 0 1 5.17-2.69" />
            <path d="M19 12.859a10 10 0 0 0-2.007-1.523" />
            <path d="M2 8.82a15 15 0 0 1 4.177-2.643" />
            <path d="M22 8.82a15 15 0 0 0-11.288-3.764" />
            <path d="m2 2 20 20" />
        </LucideIcon>
    }
}

#[component]
pub fn X(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <LucideIcon class=class>
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
        </LucideIcon>
    }
}
