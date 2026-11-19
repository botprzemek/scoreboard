use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLightState {
    NoFocus,
    Normal,
    Hover,
    Press,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrafficLightKind {
    Close,
    Minimize,
    Maximize,
}

impl TrafficLightKind {
    const fn title(self) -> &'static str {
        match self {
            Self::Close => "Zamknij",
            Self::Minimize => "Minimalizuj",
            Self::Maximize => "Maksymalizuj (przytrzymaj: pełny ekran)",
        }
    }

    const fn colors(self, pressed: bool) -> (&'static str, &'static str) {
        match (self, pressed) {
            (Self::Close, false) => ("#ff5f56", "#e0443e"),
            (Self::Close, true) => ("#bf4942", "#993833"),
            (Self::Minimize, false) => ("#ffbd2e", "#dea123"),
            (Self::Minimize, true) => ("#bf8e22", "#99711b"),
            (Self::Maximize, false) => ("#27c93f", "#1aab29"),
            (Self::Maximize, true) => ("#1d972f", "#13761c"),
        }
    }
}

fn resolve_state(focused: bool, hovered: bool, pressed: bool) -> TrafficLightState {
    if pressed {
        TrafficLightState::Press
    } else if hovered {
        TrafficLightState::Hover
    } else if focused {
        TrafficLightState::Normal
    } else {
        TrafficLightState::NoFocus
    }
}

#[allow(clippy::type_complexity)]
fn use_click_and_hold(
    onclick: Callback<web_sys::MouseEvent>,
    onhold: Callback<()>,
    hold_delay_ms: i32,
) -> (
    RwSignal<bool>,
    impl Fn(web_sys::MouseEvent) + Copy + 'static,
    impl Fn(web_sys::MouseEvent) + Copy + 'static,
    impl Fn(web_sys::MouseEvent) + Copy + 'static,
    impl Fn(web_sys::MouseEvent) + Copy + 'static,
) {
    let pressed = RwSignal::new(false);
    let timer_id = StoredValue::new(None::<i32>);
    let held = StoredValue::new(false);

    let clear_timer = move || {
        if let Some(id) = timer_id.get_value() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            timer_id.set_value(None);
        }
    };

    let on_mousedown = move |e: web_sys::MouseEvent| {
        e.stop_propagation();
        if e.button() != 0 {
            return;
        }
        pressed.set(true);
        held.set_value(false);
        clear_timer();

        let cb = Closure::wrap(Box::new(move || {
            held.set_value(true);
            onhold.run(());
        }) as Box<dyn FnMut()>);

        if let Some(w) = web_sys::window()
            && let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                hold_delay_ms,
            )
        {
            timer_id.set_value(Some(id));
        }
        cb.forget();
    };

    let on_mouseup = move |e: web_sys::MouseEvent| {
        e.stop_propagation();
        pressed.set(false);
        clear_timer();
        if e.button() == 0 && !held.get_value() {
            onclick.run(e);
        }
        held.set_value(false);
    };

    let on_mouseleave = move |e: web_sys::MouseEvent| {
        e.stop_propagation();
        pressed.set(false);
        clear_timer();
        held.set_value(false);
    };

    let on_click = move |e: web_sys::MouseEvent| {
        e.prevent_default();
        e.stop_propagation();
    };

    (pressed, on_mousedown, on_mouseup, on_mouseleave, on_click)
}

#[component]
pub(super) fn TrafficLightButton(
    kind: TrafficLightKind,
    focused: Signal<bool>,
    hovered: Signal<bool>,
    onclick: Callback<web_sys::MouseEvent>,
    onhold: Callback<()>,
    hold_delay_ms: i32,
) -> impl IntoView {
    let (pressed, on_mousedown, on_mouseup, on_mouseleave, on_click) =
        use_click_and_hold(onclick, onhold, hold_delay_ms);
    let state = Signal::derive(move || resolve_state(focused.get(), hovered.get(), pressed.get()));

    view! {
        <button
            type="button"
            title=kind.title()
            on:mousedown=on_mousedown
            on:mouseup=on_mouseup
            on:mouseleave=on_mouseleave
            on:click=on_click
            class="w-3 h-3 rounded-full flex items-center justify-center p-0 border-0 outline-none cursor-pointer bg-transparent pointer-events-auto"
        >
            {move || {
                let current = state.get();
                let (fill, stroke) = match current {
                    TrafficLightState::NoFocus => ("#4a4d52", "#3e4045"),
                    TrafficLightState::Normal | TrafficLightState::Hover => kind.colors(false),
                    TrafficLightState::Press => kind.colors(true),
                };
                let show_glyph = matches!(current, TrafficLightState::Hover | TrafficLightState::Press);
                let is_press = matches!(current, TrafficLightState::Press);

                view! {
                    <svg viewBox="0 0 12 12" class="w-3 h-3 pointer-events-none">
                        <circle cx="6" cy="6" r="5.5" fill=fill stroke=stroke stroke-width="0.5"/>
                        {show_glyph.then(|| match kind {
                            TrafficLightKind::Close => {
                                let glyph = if is_press { "#260000" } else { "#4d0000" };
                                view! {
                                    <path d="M3.8 3.8 L8.2 8.2 M8.2 3.8 L3.8 8.2" stroke=glyph stroke-width="1.2" stroke-linecap="round"/>
                                }.into_any()
                            }
                            TrafficLightKind::Minimize => {
                                let glyph = if is_press { "#2e1e00" } else { "#5c3c00" };
                                view! {
                                    <path d="M2.5 6 L9.5 6" stroke=glyph stroke-width="1.2" stroke-linecap="round"/>
                                }.into_any()
                            }
                            TrafficLightKind::Maximize => {
                                let glyph = if is_press { "#001a05" } else { "#003b0c" };
                                view! {
                                    <path d="M3.5 3.5 L7.5 3.5 L7.5 7.5 Z M8.5 8.5 L4.5 8.5 L4.5 4.5 Z" fill=glyph/>
                                }.into_any()
                            }
                        })}
                    </svg>
                }
            }}
        </button>
    }
}
