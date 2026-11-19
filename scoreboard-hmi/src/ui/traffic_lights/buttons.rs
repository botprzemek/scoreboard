use super::window_button::{TrafficLightButton, TrafficLightKind};
use leptos::prelude::*;

#[component]
pub fn CloseButton(
    focused: Signal<bool>,
    hovered: Signal<bool>,
    #[prop(into, default = Callback::new(|_| ()))] onclick: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| ()))] onhold: Callback<()>,
    #[prop(into, default = 500)] hold_delay_ms: i32,
) -> impl IntoView {
    view! {
        <TrafficLightButton
            kind=TrafficLightKind::Close
            focused=focused
            hovered=hovered
            onclick=onclick
            onhold=onhold
            hold_delay_ms=hold_delay_ms
        />
    }
}

#[component]
pub fn MinimizeButton(
    focused: Signal<bool>,
    hovered: Signal<bool>,
    #[prop(into, default = Callback::new(|_| ()))] onclick: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| ()))] onhold: Callback<()>,
    #[prop(into, default = 500)] hold_delay_ms: i32,
) -> impl IntoView {
    view! {
        <TrafficLightButton
            kind=TrafficLightKind::Minimize
            focused=focused
            hovered=hovered
            onclick=onclick
            onhold=onhold
            hold_delay_ms=hold_delay_ms
        />
    }
}

#[component]
pub fn MaximizeButton(
    focused: Signal<bool>,
    hovered: Signal<bool>,
    #[prop(into, default = Callback::new(|_| ()))] onclick: Callback<web_sys::MouseEvent>,
    #[prop(into, default = Callback::new(|_| ()))] onhold: Callback<()>,
    #[prop(into, default = 500)] hold_delay_ms: i32,
) -> impl IntoView {
    view! {
        <TrafficLightButton
            kind=TrafficLightKind::Maximize
            focused=focused
            hovered=hovered
            onclick=onclick
            onhold=onhold
            hold_delay_ms=hold_delay_ms
        />
    }
}
