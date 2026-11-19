use core::pin::pin;
use core::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use crate::adapters::tauri::invoke_discover_master;
use crate::adapters::ws::{
    BrowserWebSocketAdapter, HmiClientHandler, load_master_address, load_or_create_browser_device,
    save_master_address,
};

use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use futures::future::{self, Either};
use leptos::prelude::*;
use scoreboard_basketball::{Discipline, Game, GameMachine, Score, ScoreWrite, Team};
use scoreboard_config::DeviceConfig;
use scoreboard_net::layout::{DATA_SIZE, PACKET_SIZE};
use scoreboard_net::{
    Client, ClientEvent, ClientPacket, Device, DeviceType, NetError, ServerEvent, ServerPacket,
    Transport,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{BinaryType, MessageEvent, WebSocket};

#[derive(Clone, Copy)]
pub struct MasterContext {
    pub is_connected: RwSignal<bool>,
    pub master_address: RwSignal<String>,
    pub my_device: RwSignal<Device>,
    pub current_game: RwSignal<Game>,
    pub available_games: RwSignal<Vec<Game>>,
    pub connected_devices: RwSignal<Vec<Device>>,
    pub logs: RwSignal<Vec<String>>,
    reconnect_trigger: RwSignal<u32>,
    command_sender: RwSignal<Option<UnboundedSender<ClientPacket>>>,
    sequence_id: StoredValue<Arc<AtomicU16>>,
}

impl MasterContext {
    pub fn init() -> Self {
        let master_address = RwSignal::new(load_master_address());
        let reconnect_trigger = RwSignal::new(0u32);
        let is_connected = RwSignal::new(false);
        let command_sender = RwSignal::new(None::<UnboundedSender<ClientPacket>>);
        let sequence_id = StoredValue::new(Arc::new(AtomicU16::new(0)));
        let logs = RwSignal::new(Vec::<String>::new());

        let current_game = RwSignal::new(Game::new(1, Discipline::Fiba5v5));
        let available_games = RwSignal::new(Vec::<Game>::new());
        let connected_devices = RwSignal::new(Vec::<Device>::new());
        let my_device = RwSignal::new(
            Device::try_new(DeviceType::WebBrowser, "0000-0000", "hmi-app")
                .expect("valid browser DeviceConfig"),
        );

        let push_log = move |message: String| {
            logs.update(|buf| {
                buf.push(message);
                if buf.len() > 80 {
                    buf.remove(0);
                }
            });
        };

        // Background auto-discovery + reconnect loop
        Effect::new(move |_| {
            async fn sleep_ms(millis: i32) {
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    if let Some(window) = web_sys::window() {
                        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                            &resolve, millis,
                        );
                    }
                });
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            }

            wasm_bindgen_futures::spawn_local(async move {
                let mut check_discovery = true;

                loop {
                    sleep_ms(2500).await;

                    if !is_connected.get_untracked() {
                        if check_discovery {
                            let current = master_address.get_untracked();
                            if (current == "127.0.0.1:3000" || current.is_empty())
                                && let Ok(discovered) = invoke_discover_master(Some(2)).await
                                && !discovered.is_empty()
                                && discovered != current
                            {
                                push_log(format!("[DISCOVERY] Master: {discovered}"));
                                save_master_address(&discovered);
                                master_address.set(discovered);
                                check_discovery = false;
                                continue;
                            }
                        }
                        reconnect_trigger.update(|n| *n = n.wrapping_add(1));
                    } else {
                        check_discovery = false;
                    }
                }
            });
        });

        // WebSocket transport lifecycle
        Effect::new(move |_| {
            let _ = reconnect_trigger.get();
            let client_device = load_or_create_browser_device();
            my_device.set(client_device);

            let raw_addr = master_address.get();
            let target_addr = raw_addr.trim();
            let websocket_url =
                if target_addr.starts_with("ws://") || target_addr.starts_with("wss://") {
                    if target_addr.ends_with("/ws") {
                        target_addr.to_string()
                    } else {
                        format!("{}/ws", target_addr.trim_end_matches('/'))
                    }
                } else if let Some(stripped) = target_addr.strip_prefix("http://") {
                    format!("ws://{}/ws", stripped.trim_end_matches('/'))
                } else if let Some(stripped) = target_addr.strip_prefix("https://") {
                    format!("wss://{}/ws", stripped.trim_end_matches('/'))
                } else {
                    format!("ws://{}/ws", target_addr.trim_end_matches('/'))
                };

            push_log(format!("[WS] Łączenie: {websocket_url}"));

            if let Ok(websocket) = WebSocket::new(&websocket_url) {
                websocket.set_binary_type(BinaryType::Arraybuffer);

                let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded::<[u8; PACKET_SIZE]>();
                let (incoming_tx, incoming_rx) = mpsc::unbounded::<[u8; PACKET_SIZE]>();
                let (packet_tx, mut packet_rx) = mpsc::unbounded::<ServerPacket>();
                let (command_tx, mut command_rx) = mpsc::unbounded::<ClientPacket>();

                {
                    let command_tx_open = command_tx.clone();
                    let onopen = Closure::wrap(Box::new(move |_event: JsValue| {
                        is_connected.set(true);
                        push_log("[WS] Połączono z Masterem.".to_string());

                        let handshake = ClientPacket::new(
                            ClientEvent::HandshakeRequest,
                            0,
                            client_device.to_bytes(),
                        );
                        let _ = command_tx_open.unbounded_send(handshake);
                    }) as Box<dyn FnMut(JsValue)>);
                    websocket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                    onopen.forget();
                }

                {
                    let incoming_tx_msg = incoming_tx.clone();
                    let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
                        if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                            let array = js_sys::Uint8Array::new(&buffer);
                            if array.length() as usize == PACKET_SIZE {
                                let mut data_array = [0u8; PACKET_SIZE];
                                array.copy_to(&mut data_array);
                                let _ = incoming_tx_msg.unbounded_send(data_array);
                            }
                        }
                    })
                        as Box<dyn FnMut(MessageEvent)>);
                    websocket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                    onmessage.forget();
                }

                {
                    let onclose = Closure::wrap(Box::new(move |_event: JsValue| {
                        is_connected.set(false);
                        connected_devices.set(Vec::new());
                        available_games.set(Vec::new());
                        incoming_tx.close_channel();
                        push_log("[WS] Rozłączono z Masterem.".to_string());
                    }) as Box<dyn FnMut(JsValue)>);
                    websocket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
                    onclose.forget();
                }

                {
                    let websocket_sender = websocket.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        while let Some(bytes) = outgoing_rx.next().await {
                            let uint8_array = js_sys::Uint8Array::from(&bytes[..]);
                            if let Err(err) =
                                websocket_sender.send_with_array_buffer(&uint8_array.buffer())
                            {
                                push_log(format!("[WS BŁĄD] {err:?}"));
                            }
                        }
                    });
                }

                {
                    wasm_bindgen_futures::spawn_local(async move {
                        let mut device_list = Vec::<Device>::new();
                        let mut game_list = Vec::<Game>::new();
                        let mut last_ui_flush = 0.0f64;

                        while let Some(packet) = packet_rx.next().await {
                            let mut devices_changed = false;
                            let mut games_changed = false;

                            let mut current = Some(packet);
                            while let Some(packet) = current {
                                match packet.event() {
                                    ServerEvent::HandshakeAck => {
                                        if let Ok(game) = Game::try_from(packet) {
                                            current_game.set(game);
                                            if !game_list.iter().any(|g| g.id() == game.id()) {
                                                game_list.push(game);
                                                game_list.sort_by_key(Game::id);
                                                games_changed = true;
                                            }
                                            push_log(format!("[SESJA] Gra #{}", game.id()));
                                        }
                                    }
                                    ServerEvent::GameUpdate => {
                                        if let Ok(game) = Game::try_from(packet) {
                                            if let Some(existing) =
                                                game_list.iter_mut().find(|g| g.id() == game.id())
                                            {
                                                *existing = game;
                                            } else {
                                                game_list.push(game);
                                                game_list.sort_by_key(Game::id);
                                            }
                                            games_changed = true;

                                            let current_id = current_game.with_untracked(Game::id);
                                            if current_id == game.id() || current_id == 0 {
                                                current_game.set(game);
                                            }
                                        }
                                    }
                                    ServerEvent::GameListUpdate => {
                                        if let Ok(game) = Game::try_from(packet) {
                                            if let Some(existing) =
                                                game_list.iter_mut().find(|g| g.id() == game.id())
                                            {
                                                *existing = game;
                                            } else {
                                                game_list.push(game);
                                                game_list.sort_by_key(Game::id);
                                            }
                                            games_changed = true;
                                        }
                                    }
                                    ServerEvent::DeviceUpdate => {
                                        if let Ok(updated_dev) = Device::try_from(packet) {
                                            let is_new = if let Some(existing) =
                                                device_list.iter_mut().find(|d| {
                                                    d.serial_number() == updated_dev.serial_number()
                                                }) {
                                                let changed = *existing != updated_dev;
                                                *existing = updated_dev;
                                                changed
                                            } else {
                                                device_list.push(updated_dev);
                                                true
                                            };

                                            if is_new {
                                                devices_changed = true;
                                                push_log(format!(
                                                    "[URZĄDZENIE] {:?} ({}) -> Gra #{}",
                                                    updated_dev.device_type(),
                                                    updated_dev.serial_number().unwrap_or("-"),
                                                    updated_dev.game_id()
                                                ));
                                            }
                                        }
                                    }
                                    ServerEvent::DeviceDisconnect => {
                                        if let Ok(dc_dev) = Device::try_from(packet) {
                                            device_list.retain(|d| {
                                                d.serial_number() != dc_dev.serial_number()
                                            });
                                            devices_changed = true;
                                            push_log(format!(
                                                "[ROZŁĄCZONO] {}",
                                                dc_dev.serial_number().unwrap_or("-")
                                            ));
                                        }
                                    }
                                    _ => {}
                                }

                                current = packet_rx.try_recv().ok();
                            }

                            let now = js_sys::Date::now();
                            let should_flush = (now - last_ui_flush) >= 80.0 || current.is_none();

                            if should_flush {
                                if devices_changed {
                                    connected_devices.set(device_list.clone());
                                }
                                if games_changed {
                                    available_games.set(game_list.clone());
                                }
                                last_ui_flush = now;
                            }
                        }
                    });
                }

                {
                    let adapter = BrowserWebSocketAdapter::new(outgoing_tx, incoming_rx);
                    let transport = Transport::new(adapter, None);
                    let handler = HmiClientHandler::new(packet_tx);
                    let mut client = Client::new(transport, handler);

                    wasm_bindgen_futures::spawn_local(async move {
                        enum Step {
                            Polled(Result<ServerPacket, NetError>),
                            Command(Option<ClientPacket>),
                        }

                        loop {
                            let step = {
                                let poll_future = pin!(client.poll());
                                let send_future = pin!(command_rx.next());

                                match future::select(poll_future, send_future).await {
                                    Either::Left((res, _)) => Step::Polled(res),
                                    Either::Right((cmd, _)) => Step::Command(cmd),
                                }
                            };

                            match step {
                                Step::Polled(Ok(_)) => {}
                                Step::Polled(Err(NetError::InvalidPacket(err))) => {
                                    push_log(format!("[PAKIET BŁĄD] {err}"));
                                }
                                Step::Polled(Err(NetError::Disconnected | _)) => break,
                                Step::Command(Some(packet)) => {
                                    client.send(&packet).await;
                                }
                                Step::Command(None) => break,
                            }
                        }
                    });
                }

                command_sender.set(Some(command_tx));
            } else {
                push_log(format!("[WS] Nieprawidłowy adres: {websocket_url}"));
            }
        });

        Self {
            is_connected,
            master_address,
            my_device,
            current_game,
            available_games,
            connected_devices,
            logs,
            reconnect_trigger,
            command_sender,
            sequence_id,
        }
    }

    pub fn send_packet(&self, event: ClientEvent, payload: [u8; DATA_SIZE]) {
        if let Some(sender) = self.command_sender.get_untracked().as_ref() {
            let seq = self
                .sequence_id
                .with_value(|id| id.fetch_add(1, Ordering::Relaxed).wrapping_add(1));
            let packet = ClientPacket::new(event, seq, payload);
            if sender.unbounded_send(packet).is_ok() {
                self.logs.update(|buf| {
                    buf.push(format!("[TX] {event:?} #{seq}"));
                    if buf.len() > 80 {
                        buf.remove(0);
                    }
                });
            }
        }
    }

    pub fn connect_to(&self, address: String) {
        save_master_address(&address);
        self.master_address.set(address);
        self.reconnect();
    }

    pub fn reconnect(&self) {
        self.reconnect_trigger.update(|n| *n = n.wrapping_add(1));
    }

    pub fn select_game(&self, game_id: u64) {
        if let Some(target) = self
            .available_games
            .get_untracked()
            .into_iter()
            .find(|g| g.id() == game_id)
        {
            self.current_game.set(target);
        }
        let target_game = Game::new(game_id, Discipline::Fiba5v5);
        self.send_packet(ClientEvent::GameSelect, target_game.to_bytes());
    }

    pub fn create_game(&self, discipline: Discipline) {
        let next_id = self
            .available_games
            .get_untracked()
            .iter()
            .map(Game::id)
            .max()
            .unwrap_or(0)
            + 1;
        let new_game = Game::new(next_id, discipline);
        self.current_game.set(new_game);
        self.send_packet(ClientEvent::GameCreate, new_game.to_bytes());
    }

    pub fn delete_game(&self, id: u64) {
        let target = Game::new(id, Discipline::Fiba5v5);
        self.send_packet(ClientEvent::GameDelete, target.to_bytes());
        self.available_games
            .update(|games| games.retain(|g| g.id() != id));
    }

    pub fn send_game_event(&self, event: ClientEvent) {
        let game = self.current_game.get_untracked();

        self.send_packet(event, game.to_bytes());
    }

    pub fn write_score(&self, team: Team, score: Score) {
        let mut game = self.current_game.get_untracked();
        let event = match (team, score) {
            (Team::Home, Score::FreeThrow) => ClientEvent::GameScoreHomeFT,
            (Team::Home, Score::FieldGoal2) => ClientEvent::GameScoreHome2FG,
            (Team::Home, Score::FieldGoal3) => ClientEvent::GameScoreHome3FG,
            (Team::Away, Score::FreeThrow) => ClientEvent::GameScoreAwayFT,
            (Team::Away, Score::FieldGoal2) => ClientEvent::GameScoreAway2FG,
            (Team::Away, Score::FieldGoal3) => ClientEvent::GameScoreAway3FG,
            _ => return,
        };

        if let Some(mut machine) = GameMachine::from_in_progress(game) {
            machine.add_score(team, score);
            game = machine.into_game();
        } else if let Some(mut machine) = GameMachine::from_paused(game) {
            machine.add_score(team, score);
            game = machine.into_game();
        }

        self.current_game.set(game);
        self.send_packet(event, game.to_bytes());
    }

    pub fn assign_device_game(&self, mut device: Device, game_id: u64) {
        device.set_game_id(game_id);
        let target_sn = device.serial_number().ok().map(str::to_string);
        self.connected_devices.update(|list| {
            if let Some(sn) = target_sn.as_deref()
                && let Some(existing) = list.iter_mut().find(|d| d.serial_number().ok() == Some(sn))
            {
                existing.set_game_id(game_id);
            }
        });
        self.send_packet(ClientEvent::DeviceAssignGame, device.to_bytes());
    }

    pub fn disconnect_device(&self, device: Device) {
        self.send_packet(ClientEvent::DeviceDisconnect, device.to_bytes());
    }
}
