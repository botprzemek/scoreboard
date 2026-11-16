use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::separator::Separator;
use crate::master::MasterContext;
use basketball::{Discipline, Quarter, Score, State, Team};
use icons::{Eye, Pause, Pencil, Play, RotateCcw, Square, Terminal};
use leptos::prelude::*;
use net::ClientEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HmiRoleMode {
    #[default]
    ScoreWrite,
    ScoreRead,
}

fn format_quarter(quarter: Quarter) -> &'static str {
    match quarter {
        Quarter::None => "3x3",
        Quarter::Q1 => "Q1",
        Quarter::Q2 => "Q2",
        Quarter::Q3 => "Q3",
        Quarter::Q4 => "Q4",
        _ => "Q?",
    }
}

fn format_state(state: State) -> (&'static str, BadgeVariant) {
    match state {
        State::Idle => ("Gotowy", BadgeVariant::Secondary),
        State::Running => ("W trakcie", BadgeVariant::Success),
        State::Paused => ("Pauza", BadgeVariant::Warning),
        State::QuarterEnd => ("Koniec kwarty", BadgeVariant::Default),
        State::GameEnd => ("Koniec meczu", BadgeVariant::Destructive),
        _ => ("Nieznany", BadgeVariant::Secondary),
    }
}

#[component]
pub fn ScoreboardView(master: MasterContext, role_mode: RwSignal<HmiRoleMode>) -> impl IntoView {
    let logs_expanded = RwSignal::new(false);

    let m_start = master;
    let m_resume = master;
    let m_pause = master;
    let m_end = master;
    let m_reset = master;

    let m_home_1 = master;
    let m_home_2 = master;
    let m_home_3 = master;
    let m_away_1 = master;
    let m_away_2 = master;
    let m_away_3 = master;

    let disconnected = Signal::derive(move || !master.is_connected.get());

    view! {
        <div class="flex-1 overflow-y-auto p-3 sm:p-5 lg:p-6">
            <div class="mx-auto max-w-5xl space-y-4">
                // Top bar: Game meta + ScoreRead / ScoreWrite segmented switcher
                <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                    <div class="flex flex-wrap items-center gap-2">
                        <h1 class="text-base sm:text-lg font-semibold tracking-tight">
                            {move || format!("Mecz #{}", master.current_game.get().id())}
                        </h1>
                        <Badge variant=BadgeVariant::Outline>
                            {move || match master.current_game.get().discipline() {
                                Discipline::Fiba5v5 => "FIBA 5v5",
                                Discipline::Fiba3v3 => "FIBA 3v3",
                                _ => "FIBA",
                            }}
                        </Badge>
                        <Badge variant=BadgeVariant::Secondary>
                            {move || format_quarter(master.current_game.get().quarter())}
                        </Badge>
                        {move || {
                            let (label, variant) = format_state(master.current_game.get().state());
                            view! { <Badge variant=variant>{label}</Badge> }
                        }}
                    </div>

                    // macOS-style segmented control for ScoreRead vs ScoreWrite
                    <div class="inline-flex self-start rounded-xl border border-border bg-secondary/70 p-1 backdrop-blur-md">
                        <Button
                            variant=Signal::derive(move || {
                                if role_mode.get() == HmiRoleMode::ScoreRead {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Ghost
                                }
                            })
                            size=ButtonSize::Sm
                            class="rounded-lg"
                            on:click=move |_| role_mode.set(HmiRoleMode::ScoreRead)
                        >
                            <Eye class="h-3.5 w-3.5" />
                            <span>"ScoreRead"</span>
                        </Button>
                        <Button
                            variant=Signal::derive(move || {
                                if role_mode.get() == HmiRoleMode::ScoreWrite {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Ghost
                                }
                            })
                            size=ButtonSize::Sm
                            class="rounded-lg"
                            on:click=move |_| role_mode.set(HmiRoleMode::ScoreWrite)
                        >
                            <Pencil class="h-3.5 w-3.5" />
                            <span>"ScoreWrite"</span>
                        </Button>
                    </div>
                </div>

                // ScoreRead Main Scoreboard Card (always visible in both ScoreRead and ScoreWrite)
                <Card class="overflow-hidden">
                    <CardContent class="p-4 sm:p-6">
                        <div class="grid grid-cols-1 gap-4 sm:grid-cols-3 sm:items-center">
                            // HOME ScoreRead
                            <div class="flex flex-col items-center justify-center rounded-xl bg-muted/60 border border-border/60 p-4 sm:p-5">
                                <span class="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                                    "Gospodarze (Home)"
                                </span>
                                <div class="mt-2 font-mono text-5xl sm:text-6xl font-bold tracking-tight tabular-nums text-foreground">
                                    {move || MasterContext::read_scores(&master.current_game.get()).home}
                                </div>
                            </div>

                            // Center Clocks (Regulation + 24s Shot Clock)
                            <div class="flex flex-col items-center justify-center gap-3 rounded-xl bg-background/60 border border-border/60 p-4 sm:p-5">
                                <div class="flex flex-col items-center">
                                    <span class="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground">
                                        "Czas gry"
                                    </span>
                                    <div class="mt-1 font-mono text-3xl sm:text-4xl font-bold tracking-tight tabular-nums">
                                        {move || {
                                            let reg = master.current_game.get().regulation_time();
                                            if reg.minutes == 0 {
                                                format!("{:02}.{:02}", reg.seconds, reg.centiseconds)
                                            } else {
                                                format!("{:02}:{:02}", reg.minutes, reg.seconds)
                                            }
                                        }}
                                    </div>
                                </div>

                                <Separator class="w-24" />

                                <div class="flex flex-col items-center">
                                    <span class="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground">
                                        "Zegar akcji"
                                    </span>
                                    <div class="mt-0.5 font-mono text-2xl font-bold tabular-nums text-amber-500">
                                        {move || {
                                            let clk = master.current_game.get().clock_time();
                                            format!("{:02}.{}", clk.seconds, clk.centiseconds / 10)
                                        }}
                                    </div>
                                </div>
                            </div>

                            // AWAY ScoreRead
                            <div class="flex flex-col items-center justify-center rounded-xl bg-muted/60 border border-border/60 p-4 sm:p-5">
                                <span class="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                                    "Goście (Away)"
                                </span>
                                <div class="mt-2 font-mono text-5xl sm:text-6xl font-bold tracking-tight tabular-nums text-foreground">
                                    {move || MasterContext::read_scores(&master.current_game.get()).away}
                                </div>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                // ScoreWrite Controls (hidden in pure ScoreRead mode)
                <Show when=move || role_mode.get() == HmiRoleMode::ScoreWrite>
                    <div class="space-y-4">
                        // Match Clock & State Controls
                        <Card>
                            <CardHeader class="pb-3">
                                <CardTitle>"Sterowanie zegarem i meczem"</CardTitle>
                                <CardDescription>
                                    "Wysyłanie zdarzeń czasu rzeczywistego do Mastera"
                                </CardDescription>
                            </CardHeader>
                            <CardContent>
                                <div class="flex flex-wrap gap-2">
                                    <Button
                                        variant=ButtonVariant::Success
                                        size=ButtonSize::Lg
                                        disabled=disconnected
                                        on:click=move |_| m_start.send_game_event(ClientEvent::GameStart)
                                    >
                                        <Play class="h-4 w-4" />
                                        <span>"Start"</span>
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Default
                                        size=ButtonSize::Lg
                                        disabled=disconnected
                                        on:click=move |_| m_resume.send_game_event(ClientEvent::GameResume)
                                    >
                                        <Play class="h-4 w-4" />
                                        <span>"Wznów"</span>
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Outline
                                        size=ButtonSize::Lg
                                        disabled=disconnected
                                        on:click=move |_| m_pause.send_game_event(ClientEvent::GamePause)
                                    >
                                        <Pause class="h-4 w-4" />
                                        <span>"Pauza"</span>
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        size=ButtonSize::Lg
                                        disabled=disconnected
                                        on:click=move |_| m_end.send_game_event(ClientEvent::GameEnd)
                                    >
                                        <Square class="h-4 w-4" />
                                        <span>"Koniec"</span>
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Destructive
                                        size=ButtonSize::Lg
                                        disabled=disconnected
                                        on:click=move |_| m_reset.send_game_event(ClientEvent::GameReset)
                                    >
                                        <RotateCcw class="h-4 w-4" />
                                        <span>"Reset"</span>
                                    </Button>
                                </div>
                            </CardContent>
                        </Card>

                        // ScoreWrite Panels (Home & Away)
                        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                            <Card>
                                <CardHeader class="pb-3">
                                    <CardTitle>"ScoreWrite — Gospodarze (Home)"</CardTitle>
                                    <CardDescription>"Dodaj punkty drużynie gospodarzy"</CardDescription>
                                </CardHeader>
                                <CardContent>
                                    <div class="grid grid-cols-3 gap-2.5">
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_home_1.write_score(Team::Home, Score::FreeThrow)
                                        >
                                            "+1 FT"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Default
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_home_2.write_score(Team::Home, Score::FieldGoal2)
                                        >
                                            "+2 FG"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Default
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_home_3.write_score(Team::Home, Score::FieldGoal3)
                                        >
                                            "+3 FG"
                                        </Button>
                                    </div>
                                </CardContent>
                            </Card>

                            <Card>
                                <CardHeader class="pb-3">
                                    <CardTitle>"ScoreWrite — Goście (Away)"</CardTitle>
                                    <CardDescription>"Dodaj punkty drużynie gości"</CardDescription>
                                </CardHeader>
                                <CardContent>
                                    <div class="grid grid-cols-3 gap-2.5">
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_away_1.write_score(Team::Away, Score::FreeThrow)
                                        >
                                            "+1 FT"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Default
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_away_2.write_score(Team::Away, Score::FieldGoal2)
                                        >
                                            "+2 FG"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Default
                                            size=ButtonSize::Lg
                                            class="h-12 text-base font-semibold"
                                            disabled=disconnected
                                            on:click=move |_| m_away_3.write_score(Team::Away, Score::FieldGoal3)
                                        >
                                            "+3 FG"
                                        </Button>
                                    </div>
                                </CardContent>
                            </Card>
                        </div>
                    </div>
                </Show>

                // Collapsible Telemetry / Log Card
                <Card>
                    <CardHeader
                        class="py-3 cursor-pointer flex-row items-center justify-between"
                    >
                        <div
                            class="flex flex-1 items-center justify-between"
                            on:click=move |_| logs_expanded.update(|v| *v = !*v)
                        >
                            <div class="flex items-center gap-2">
                                <Terminal class="h-4 w-4 text-muted-foreground" />
                                <CardTitle>"Logi połączenia z Masterem"</CardTitle>
                            </div>
                            <Badge variant=BadgeVariant::Secondary>
                                {move || master.logs.get().len()}
                            </Badge>
                        </div>
                    </CardHeader>
                    <Show when=move || logs_expanded.get()>
                        <CardContent>
                            <div class="max-h-44 overflow-y-auto rounded-lg bg-muted/70 border border-border p-3 font-mono text-[11px] space-y-1 text-muted-foreground">
                                {move || {
                                    let entries = master.logs.get();
                                    if entries.is_empty() {
                                        view! { <div>"Brak zdarzeń."</div> }.into_any()
                                    } else {
                                        entries
                                            .into_iter()
                                            .rev()
                                            .map(|line| view! { <div class="truncate">{line}</div> })
                                            .collect_view()
                                            .into_any()
                                    }
                                }}
                            </div>
                        </CardContent>
                    </Show>
                </Card>
            </div>
        </div>
    }
}
