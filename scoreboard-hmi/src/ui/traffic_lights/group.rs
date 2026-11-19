use super::buttons::{CloseButton, MaximizeButton, MinimizeButton};
use crate::adapters::tauri::invoke_tauri_window_command;
use leptos::prelude::*;

#[component]
pub fn TrafficLights(
    #[prop(into, default = Signal::derive(|| true))] focused: Signal<bool>,
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
) -> impl IntoView {
    let hovered = RwSignal::new(false);

    view! {
        <div
            on:mouseenter=move |_| hovered.set(true)
            on:mouseleave=move |_| hovered.set(false)
            class="flex items-center gap-2 pointer-events-auto"
        >
            <CloseButton
                focused=focused
                hovered=hovered.into()
                onclick=on_close
                onhold=on_hold_close
                hold_delay_ms=hold_delay_ms
            />
            <MinimizeButton
                focused=focused
                hovered=hovered.into()
                onclick=on_minimize
                onhold=on_hold_minimize
                hold_delay_ms=hold_delay_ms
            />
            <MaximizeButton
                focused=focused
                hovered=hovered.into()
                onclick=on_maximize
                onhold=on_hold_maximize
                hold_delay_ms=hold_delay_ms
            />
        </div>
    }
}
