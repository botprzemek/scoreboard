use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::separator::Separator;
use crate::master::MasterContext;
use crate::components::icons::{Cpu, Monitor, Plus, Trash2, Trophy, X};
use leptos::prelude::*;
use scoreboard_basketball::{Discipline, Game, ScoreRead, State};
use scoreboard_config::DeviceConfig;
use scoreboard_net::DeviceType;

fn state_badge_variant(state: State) -> BadgeVariant {
    match state {
        State::InProgress => BadgeVariant::Success,
        State::Paused => BadgeVariant::Warning,
        State::QuarterEnd => BadgeVariant::Default,
        State::Final => BadgeVariant::Destructive,
        State::Scheduled | State::QuarterStart | _ => BadgeVariant::Secondary,
    }
}

fn state_label(state: State) -> &'static str {
    match state {
        State::Scheduled => "Idle",
        State::QuarterStart => "Start Q",
        State::InProgress => "Live",
        State::Paused => "Pauza",
        State::QuarterEnd => "Koniec Q",
        State::Final => "Koniec",
        _ => "Nieznany",
    }
}

fn discipline_label(discipline: Discipline) -> &'static str {
    match discipline {
        Discipline::Fiba5v5 => "5v5",
        Discipline::Fiba3v3 => "3v3",
        _ => "FIBA",
    }
}

#[component]
pub fn sidebar_panel(
    master: MasterContext,
    #[prop(optional)] on_close_mobile: Option<Callback<()>>,
) -> impl IntoView {
    let master_5v5 = master;
    let master_3v3 = master;
    let master_games = master;
    let master_devices = master;

    let show_ota = RwSignal::new(false);
    let ota_ip = RwSignal::new("192.168.4.1".to_string());
    let ota_sn = RwSignal::new(String::new());
    let ota_id = RwSignal::new(String::new());
    let ota_ssid = RwSignal::new(String::new());
    let ota_pass = RwSignal::new(String::new());
    let ota_busy = RwSignal::new(false);
    let ota_status = RwSignal::new(Option::<Result<String, String>>::None);

    view! {
        <aside class="flex h-full w-full flex-col bg-card/75 backdrop-blur-xl border-r border-border">
            <div class="flex items-center justify-between px-3.5 py-2.5">
                <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                    <Trophy class="h-3.5 w-3.5" />
                    <span>"Gry"</span>
                </div>
                <div class="flex items-center gap-1">
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        disabled=Signal::derive(move || !master_5v5.is_connected.get())
                        on:click=move |_| master_5v5.create_game(Discipline::Fiba5v5)
                    >
                        <Plus class="h-3 w-3" />
                        <span>"5v5"</span>
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        size=ButtonSize::Sm
                        disabled=Signal::derive(move || !master_3v3.is_connected.get())
                        on:click=move |_| master_3v3.create_game(Discipline::Fiba3v3)
                    >
                        <Plus class="h-3 w-3" />
                        <span>"3v3"</span>
                    </Button>
                    {on_close_mobile.map(|close_cb| {
                        view! {
                            <Button
                                variant=ButtonVariant::Ghost
                                size=ButtonSize::Icon
                                class="md:hidden h-7 w-7"
                                on:click=move |_| close_cb.run(())
                            >
                                <X class="h-4 w-4" />
                            </Button>
                        }
                    })}
                </div>
            </div>

            <div class="flex-1 overflow-y-auto px-2.5 pb-3 space-y-1">
                {move || {
                    let games = master_games.available_games.get();
                    let active_id = master_games.current_game.get().id();
                    let can_delete = games.len() > 1;

                    if games.is_empty() {
                        return view! {
                            <div class="rounded-lg border border-dashed border-border p-3 text-center text-xs text-muted-foreground">
                                "Brak gier (połącz z Masterem)"
                            </div>
                        }.into_any();
                    }

                    games
                        .into_iter()
                        .map(|game: Game| {
                            let gid = game.id();
                            let is_active = gid == active_id;
                            let scores = game.scores();
                            let m_select = master_games;
                            let m_delete = master_games;
                            let close_cb = on_close_mobile;

                            let row_class = if is_active {
                                "group flex items-center justify-between gap-2 rounded-lg bg-primary/12 border border-primary/30 px-2.5 py-2 cursor-pointer transition-colors"
                            } else {
                                "group flex items-center justify-between gap-2 rounded-lg border border-transparent hover:bg-secondary/80 px-2.5 py-2 cursor-pointer transition-colors"
                            };

                            view! {
                                <div
                                    class=row_class
                                    on:click=move |_| {
                                        m_select.select_game(gid);
                                        if let Some(cb) = close_cb {
                                            cb.run(());
                                        }
                                    }
                                >
                                    <div class="min-w-0 flex-1">
                                        <div class="flex items-center gap-1.5">
                                            <span class="text-xs font-semibold truncate">
                                                {format!("Gra #{gid}")}
                                            </span>
                                            <Badge variant=BadgeVariant::Outline class="px-1.5 py-0 text-[10px]">
                                                {discipline_label(game.discipline())}
                                            </Badge>
                                        </div>
                                        <div class="mt-1 flex items-center gap-2 text-[11px] text-muted-foreground">
                                            <span class="font-mono font-semibold text-foreground tabular-nums">
                                                {format!("{}:{}", scores.0, scores.1)}
                                            </span>
                                            <Badge variant=state_badge_variant(game.state()) class="px-1.5 py-0 text-[10px]">
                                                {state_label(game.state())}
                                            </Badge>
                                        </div>
                                    </div>
                                    <Show when=move || can_delete>
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Icon
                                            class="h-7 w-7 opacity-60 hover:opacity-100 hover:text-destructive"
                                            title="Usuń grę"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                m_delete.delete_game(gid);
                                            }
                                        >
                                            <Trash2 class="h-3.5 w-3.5" />
                                        </Button>
                                    </Show>
                                </div>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </div>

            <Separator />

            <div class="px-3.5 py-2.5 flex items-center justify-between">
                <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                    <Cpu class="h-3.5 w-3.5" />
                    <span>"Urządzenia"</span>
                </div>
                <div class="flex items-center gap-1.5">
                    <button
                        type="button"
                        class="rounded px-1.5 py-0.5 text-[10px] font-semibold border border-border bg-secondary/50 hover:bg-secondary text-foreground transition-colors cursor-pointer"
                        on:click=move |_| show_ota.update(|v| *v = !*v)
                    >
                        "OTA"
                    </button>
                    <Badge variant=BadgeVariant::Secondary class="px-1.5 py-0 text-[10px]">
                        {move || master_devices.connected_devices.get().len()}
                    </Badge>
                </div>
            </div>

            <Show when=move || show_ota.get()>
                <div class="mx-2.5 mb-2.5 rounded-lg border border-border bg-card/80 p-2.5 space-y-2">
                    <div class="text-[11px] font-semibold text-foreground">"Konfiguracja OTA workera"</div>
                    <div class="grid grid-cols-2 gap-1.5">
                        <input
                            type="text"
                            placeholder="IP (np. 192.168.4.1)"
                            class="col-span-2 rounded border border-border bg-background px-2 py-1 text-[11px] text-foreground"
                            prop:value=move || ota_ip.get()
                            on:input=move |ev| ota_ip.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="SN (np. 0000-0001)"
                            class="rounded border border-border bg-background px-2 py-1 text-[11px] text-foreground"
                            prop:value=move || ota_sn.get()
                            on:input=move |ev| ota_sn.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="ID (np. worker-1)"
                            class="rounded border border-border bg-background px-2 py-1 text-[11px] text-foreground"
                            prop:value=move || ota_id.get()
                            on:input=move |ev| ota_id.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="Wi-Fi SSID"
                            class="rounded border border-border bg-background px-2 py-1 text-[11px] text-foreground"
                            prop:value=move || ota_ssid.get()
                            on:input=move |ev| ota_ssid.set(event_target_value(&ev))
                        />
                        <input
                            type="password"
                            placeholder="Hasło Wi-Fi"
                            class="rounded border border-border bg-background px-2 py-1 text-[11px] text-foreground"
                            prop:value=move || ota_pass.get()
                            on:input=move |ev| ota_pass.set(event_target_value(&ev))
                        />
                    </div>
                    <button
                        type="button"
                        disabled=move || ota_busy.get()
                        class="w-full rounded bg-primary px-2.5 py-1 text-[11px] font-semibold text-primary-foreground hover:opacity-90 disabled:opacity-50 cursor-pointer"
                        on:click=move |_| {
                            if ota_busy.get_untracked() {
                                return;
                            }
                            ota_busy.set(true);
                            ota_status.set(None);
                            let ip = ota_ip.get_untracked();
                            let sn = ota_sn.get_untracked();
                            let id = ota_id.get_untracked();
                            let ssid = ota_ssid.get_untracked();
                            let pass = ota_pass.get_untracked();
                            leptos::task::spawn_local(async move {
                                let res = crate::adapters::tauri::invoke_send_device_config(
                                    ip, sn, id, ssid, pass,
                                )
                                .await;
                                ota_status.set(Some(res));
                                ota_busy.set(false);
                            });
                        }
                    >
                        {move || if ota_busy.get() { "Wysyłanie..." } else { "Wyślij konfigurację" }}
                    </button>
                    {move || match ota_status.get() {
                        Some(Ok(msg)) => view! {
                            <div class="text-[10px] text-emerald-500">{msg}</div>
                        }.into_any(),
                        Some(Err(err)) => view! {
                            <div class="text-[10px] text-destructive">{err}</div>
                        }.into_any(),
                        None => view! { <span /> }.into_any(),
                    }}
                </div>
            </Show>

            <div class="max-h-56 overflow-y-auto px-2.5 pb-3 space-y-1">
                {move || {
                    let devices = master_devices.connected_devices.get();
                    let active_gid = master_devices.current_game.get().id();
                    let self_sn = master_devices
                        .my_device
                        .get()
                        .serial_number()
                        .unwrap_or("")
                        .to_string();

                    if devices.is_empty() {
                        return view! {
                            <div class="rounded-lg border border-dashed border-border p-2.5 text-center text-[11px] text-muted-foreground">
                                "Brak podłączonych urządzeń"
                            </div>
                        }.into_any();
                    }

                    let games_snapshot = master_devices.available_games.get();
                    devices
                        .into_iter()
                        .map(|dev| {
                            let sn = dev.serial_number().unwrap_or("-").to_string();
                            let dev_id = dev.device_id().unwrap_or("device").to_string();
                            let dev_gid = dev.game_id();
                            let is_self = sn == self_sn;
                            let is_worker = dev.device_type() == DeviceType::Worker;
                            let m_assign = master_devices;
                            let m_select = master_devices;
                            let m_dc = master_devices;
                            let game_options = games_snapshot.clone();

                            view! {
                                <div class="flex flex-col gap-1.5 rounded-lg bg-muted/60 px-2.5 py-2 border border-border/60">
                                    <div class="flex items-center justify-between gap-2">
                                        <div class="min-w-0 flex-1">
                                            <div class="flex items-center gap-1.5">
                                                {if is_worker {
                                                    view! { <Cpu class="h-3.5 w-3.5 text-primary shrink-0" /> }.into_any()
                                                } else {
                                                    view! { <Monitor class="h-3.5 w-3.5 text-muted-foreground shrink-0" /> }.into_any()
                                                }}
                                                <span class="text-xs font-medium truncate">{dev_id}</span>
                                            </div>
                                            <div class="mt-0.5 flex items-center gap-1.5 text-[10px] text-muted-foreground font-mono">
                                                <span>{sn}</span>
                                                <span>"•"</span>
                                                <span>{format!("Gra #{dev_gid}")}</span>
                                            </div>
                                        </div>
                                        <Show when=move || !is_self>
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                size=ButtonSize::Icon
                                                class="h-6 w-6 shrink-0 hover:text-destructive"
                                                title="Rozłącz urządzenie"
                                                on:click=move |_| m_dc.disconnect_device(dev)
                                            >
                                                <X class="h-3 w-3" />
                                            </Button>
                                        </Show>
                                    </div>

                                    <div class="flex items-center gap-1.5">
                                        <select
                                            class="h-7 flex-1 rounded-md border border-input bg-background/90 px-2 text-[11px] text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                            prop:value=dev_gid.to_string()
                                            on:change=move |ev| {
                                                if let Ok(new_gid) = event_target_value(&ev).parse::<u64>() {
                                                    m_select.assign_device_game(dev, new_gid);
                                                }
                                            }
                                        >
                                            <option value="0" selected=dev_gid == 0>"Brak gry (#0)"</option>
                                            {game_options
                                                .iter()
                                                .map(|g| {
                                                    let gid = g.id();
                                                    view! {
                                                        <option value=gid.to_string() selected=dev_gid == gid>
                                                            {format!("Gra #{gid} ({})", discipline_label(g.discipline()))}
                                                        </option>
                                                    }
                                                })
                                                .collect_view()}
                                        </select>
                                        <Show when=move || dev_gid != active_gid>
                                            <Button
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                                class="h-7 px-2 text-[10px] shrink-0"
                                                title="Przypisz do aktywnej gry"
                                                on:click=move |_| m_assign.assign_device_game(dev, active_gid)
                                            >
                                                {format!("→ #{active_gid}")}
                                            </Button>
                                        </Show>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </div>
        </aside>
    }
}
