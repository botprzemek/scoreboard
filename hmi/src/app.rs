use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::master::MasterContext;
use crate::ui::master_dialog::MasterDialog;
use crate::ui::scoreboard_view::{HmiRoleMode, ScoreboardView};
use crate::ui::sidebar_panel::SidebarPanel;
use crate::ui::traffic_lights::NavigationBar;
use icons::{Menu, Server, Wifi, WifiOff};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let is_focused = RwSignal::new(true);
    let is_master_modal_open = RwSignal::new(false);
    let is_mobile_sidebar_open = RwSignal::new(false);
    let role_mode = RwSignal::new(HmiRoleMode::ScoreWrite);

    let master = MasterContext::init();

    Effect::new(move |_| {
        if let Some(window) = web_sys::window() {
            let on_focus = Closure::<dyn FnMut()>::new(move || {
                is_focused.set(true);
            });
            let on_blur = Closure::<dyn FnMut()>::new(move || {
                is_focused.set(false);
            });
            let _ =
                window.add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref());
            let _ =
                window.add_event_listener_with_callback("blur", on_blur.as_ref().unchecked_ref());
            on_focus.forget();
            on_blur.forget();

            if let Ok(tauri_obj) = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
                && let Ok(event_obj) = js_sys::Reflect::get(&tauri_obj, &JsValue::from_str("event"))
                && let Ok(listen_fn) =
                    js_sys::Reflect::get(&event_obj, &JsValue::from_str("listen"))
                && let Ok(func) = listen_fn.dyn_into::<js_sys::Function>()
            {
                let on_tauri_focus = Closure::<dyn FnMut(JsValue)>::new(move |val: JsValue| {
                    if let Ok(payload) = js_sys::Reflect::get(&val, &JsValue::from_str("payload"))
                        && let Some(focused) = payload.as_bool()
                    {
                        is_focused.set(focused);
                    }
                });
                let _ = func.call2(
                    &event_obj,
                    &JsValue::from_str("hmi-focus-changed"),
                    on_tauri_focus.as_ref().unchecked_ref(),
                );
                on_tauri_focus.forget();
            }
        }
    });

    let master_nav = master;
    let master_desktop_sidebar = master;
    let master_mobile_sidebar = master;
    let master_main = master;

    view! {
        <div class="w-screen h-screen flex flex-col bg-background text-foreground select-none overflow-hidden font-sans">
            <NavigationBar
                focused=is_focused
                title="rp235x-hmi"
                right_action=view! {
                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Ghost
                            size=ButtonSize::Icon
                            class="md:hidden h-7 w-7"
                            title="Gry i urządzenia"
                            on:click=move |_| is_mobile_sidebar_open.update(|v| *v = !*v)
                        >
                            <Menu class="h-4 w-4" />
                        </Button>

                        <button
                            type="button"
                            on:click=move |_| is_master_modal_open.set(true)
                            class="cursor-pointer active:scale-95 transition-transform"
                        >
                            <Badge
                                variant=Signal::derive(move || {
                                    if master_nav.is_connected.get() {
                                        BadgeVariant::Success
                                    } else {
                                        BadgeVariant::Destructive
                                    }
                                })
                                class="px-2.5 py-1 gap-1.5"
                            >
                                {move || {
                                    if master_nav.is_connected.get() {
                                        view! { <Wifi class="h-3 w-3" /> }.into_any()
                                    } else {
                                        view! { <WifiOff class="h-3 w-3" /> }.into_any()
                                    }
                                }}
                                <span class="hidden sm:inline font-mono">
                                    {move || master_nav.master_address.get()}
                                </span>
                                <span class="sm:hidden">
                                    {move || if master_nav.is_connected.get() { "Online" } else { "Offline" }}
                                </span>
                            </Badge>
                        </button>

                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            class="hidden sm:inline-flex"
                            on:click=move |_| is_master_modal_open.set(true)
                        >
                            <Server class="h-3.5 w-3.5" />
                            <span>"Master"</span>
                        </Button>
                    </div>
                }.into_any()
            />

            <div class="relative flex flex-1 min-h-0 overflow-hidden">
                // Desktop macOS Sidebar
                <div class="hidden md:block w-64 lg:w-72 shrink-0 h-full">
                    <SidebarPanel master=master_desktop_sidebar />
                </div>

                // Mobile Slide-over Sidebar Drawer
                <Show when=move || is_mobile_sidebar_open.get()>
                    <div
                        class="fixed inset-0 z-40 flex md:hidden bg-black/40 backdrop-blur-xs"
                        on:click=move |_| is_mobile_sidebar_open.set(false)
                    >
                        <div
                            class="w-72 max-w-[85vw] h-full shadow-2xl"
                            on:click=move |ev| ev.stop_propagation()
                        >
                            <SidebarPanel
                                master=master_mobile_sidebar
                                on_close_mobile=Callback::new(move |()| is_mobile_sidebar_open.set(false))
                            />
                        </div>
                    </div>
                </Show>

                <ScoreboardView
                    master=master_main
                    role_mode=role_mode
                />
            </div>

            <MasterDialog
                open=is_master_modal_open
                master=master
            />
        </div>
    }
}
