use super::group::TrafficLights;
use crate::adapters::tauri::invoke_tauri_window_command;
use leptos::prelude::*;

#[component]
pub fn NavigationBar(
    #[prop(into, default = Signal::derive(|| true))] focused: Signal<bool>,
    #[prop(into, optional)] title: Option<&'static str>,
    #[prop(into, default = Callback::new(|_| invoke_tauri_window_command("window_close")))]
    on_close: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| invoke_tauri_window_command("window_minimize")))]
    on_minimize: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| invoke_tauri_window_command("window_toggle_maximize")))]
    on_maximize: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| ()))] on_hold_close: Callback<()>,
    #[prop(into, default = Callback::new(|_| ()))] on_hold_minimize: Callback<()>,
    #[prop(into, default = Callback::new(|_| invoke_tauri_window_command("window_toggle_fullscreen")))]
    on_hold_maximize: Callback<()>,
    #[prop(into, default = 500)] hold_delay_ms: i32,
    #[prop(optional)] right_action: Option<AnyView>,
) -> impl IntoView {
    let on_start_drag = move |e: web_sys::MouseEvent| {
        if e.button() == 0 {
            invoke_tauri_window_command("window_start_drag");
        }
    };

    view! {
        <header
            class="h-9 w-full flex items-center justify-between px-3.5 select-none border-b border-black/10 dark:border-white/[0.08] bg-[#f6f6f6] dark:bg-[#1e1e24] text-neutral-800 dark:text-neutral-200 transition-colors shrink-0 relative"
        >
            <div class="flex items-center relative z-30 cursor-default">
                <TrafficLights
                    focused=focused
                    on_close=on_close
                    on_minimize=on_minimize
                    on_maximize=on_maximize
                    on_hold_close=on_hold_close
                    on_hold_minimize=on_hold_minimize
                    on_hold_maximize=on_hold_maximize
                    hold_delay_ms=hold_delay_ms
                />
            </div>

            <div
                data-tauri-drag-region
                on:mousedown=on_start_drag
                class="flex-1 h-full flex items-center justify-center cursor-grab active:cursor-grabbing px-4 relative z-10"
            >
                {title.map(|t| view! {
                    <div class="pointer-events-none text-[12px] font-medium tracking-tight opacity-75">
                        {t}
                    </div>
                })}
            </div>

            <div class="shrink-0 flex items-center justify-end min-w-14">
                {right_action}
            </div>
        </header>
    }
}
