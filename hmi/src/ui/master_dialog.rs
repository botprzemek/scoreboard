use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{
    Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle,
};
use crate::components::ui::input::Input;
use crate::master::MasterContext;
use config::DeviceConfig;
use icons::{RefreshCw, Server, X};
use leptos::prelude::*;

#[component]
pub fn MasterDialog(open: RwSignal<bool>, master: MasterContext) -> impl IntoView {
    let draft_addr = RwSignal::new(master.master_address.get_untracked());

    Effect::new(move |_| {
        if open.get() {
            draft_addr.set(master.master_address.get_untracked());
        }
    });

    let master_save = master;
    let master_reconnect = master;

    view! {
        <Show when=move || open.get()>
            <div
                class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-xs p-4"
                on:click=move |_| open.set(false)
            >
                <div
                    class="w-full max-w-md"
                    on:click=move |ev| ev.stop_propagation()
                >
                    <Card class="shadow-2xl">
                        <CardHeader class="flex-row items-start justify-between gap-2">
                            <div class="flex items-center gap-2.5">
                                <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/15 text-primary">
                                    <Server class="h-4 w-4" />
                                </div>
                                <div class="flex flex-col gap-0.5">
                                    <CardTitle>"Połączenie z Masterem"</CardTitle>
                                    <CardDescription>
                                        "Adres WebSocket serwera tablicy wyników (host:port)"
                                    </CardDescription>
                                </div>
                            </div>
                            <Button
                                variant=ButtonVariant::Ghost
                                size=ButtonSize::Icon
                                on:click=move |_| open.set(false)
                            >
                                <X class="h-4 w-4" />
                            </Button>
                        </CardHeader>

                        <CardContent class="space-y-3">
                            <Input
                                value=draft_addr
                                placeholder="127.0.0.1:3000"
                                on_input=Callback::new(move |val| draft_addr.set(val))
                            />
                            <div class="flex items-center justify-between text-[11px] text-muted-foreground">
                                <span>"Urządzenie HMI:"</span>
                                <span class="font-mono">
                                    {move || {
                                        let dev = master.my_device.get();
                                        format!(
                                            "{} ({})",
                                            dev.device_id().unwrap_or("hmi-app"),
                                            dev.serial_number().unwrap_or("-")
                                        )
                                    }}
                                </span>
                            </div>
                        </CardContent>

                        <CardFooter class="justify-between gap-2">
                            <Button
                                variant=ButtonVariant::Outline
                                on:click=move |_| {
                                    master_reconnect.reconnect();
                                    open.set(false);
                                }
                            >
                                <RefreshCw class="h-3.5 w-3.5" />
                                <span>"Połącz ponownie"</span>
                            </Button>
                            <div class="flex items-center gap-2">
                                <Button
                                    variant=ButtonVariant::Ghost
                                    on:click=move |_| open.set(false)
                                >
                                    "Anuluj"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Default
                                    on:click=move |_| {
                                        let addr = draft_addr.get_untracked().trim().to_string();
                                        if !addr.is_empty() {
                                            master_save.connect_to(addr);
                                            open.set(false);
                                        }
                                    }
                                >
                                    "Zapisz i połącz"
                                </Button>
                            </div>
                        </CardFooter>
                    </Card>
                </div>
            </div>
        </Show>
    }
}
