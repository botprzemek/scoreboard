use core::net::SocketAddr;

use super::websocket::Session;
use crate::services::{DeviceService, GameService};

use basketball::{Discipline, Game, Score, Team};
use config::DeviceConfig;
use net::{
    ClientEvent, ClientPacket, Device, DeviceType, ServerEvent, ServerHandler, ServerPacket,
};

pub(crate) struct PacketHandler {
    game: GameService,
    devices: DeviceService,
    sequence_id: u16,
    validated_device: Option<Device>,
    selected_game_id: u64,
}

impl PacketHandler {
    #[must_use]
    pub fn new(game: GameService, devices: DeviceService) -> Self {
        Self {
            game,
            devices,
            sequence_id: 0,
            validated_device: None,
            selected_game_id: 0,
        }
    }

    #[must_use]
    pub fn is_validated(&self) -> bool {
        self.validated_device.is_some()
    }

    #[must_use]
    pub fn can_control_game(&self) -> bool {
        matches!(
            self.validated_device,
            Some(device) if device.device_type() == DeviceType::WebBrowser
        )
    }

    #[must_use]
    pub fn validated_device(&self) -> Option<Device> {
        self.validated_device
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn selected_game_id(&self) -> u64 {
        self.selected_game_id
    }

    #[must_use]
    pub fn session(&self) -> Session {
        Session {
            validated: self.is_validated(),
            browser: self.can_control_game(),
            game_id: self.selected_game_id,
        }
    }

    #[must_use]
    pub fn is_device(&self, serial_number: &str) -> bool {
        self.validated_device
            .is_some_and(|device| device.serial_number() == Ok(serial_number))
    }

    pub fn set_selected_game_id(&mut self, game_id: u64) {
        self.selected_game_id = game_id;
        if let Some(mut device) = self.validated_device {
            device.set_game_id(game_id);
            self.validated_device = Some(device);
        }
    }

    pub fn sync_assignment(&mut self) {
        if let Some(device) = self.validated_device
            && let Ok(serial_number) = device.serial_number()
            && let Some(assigned) = self.devices.game_id_by_serial(serial_number)
            && assigned != self.selected_game_id
        {
            self.set_selected_game_id(assigned);
        }
    }

    pub fn selected_game(&mut self) -> Option<Game> {
        if self.selected_game_id == 0 {
            self.sync_assignment();
            if self.selected_game_id == 0 {
                return None;
            }
        }
        self.game.by_id(self.selected_game_id)
    }

    pub fn register_device(
        devices: &DeviceService,
        game: &GameService,
        mut device: Device,
        preferred_game_id: u64,
    ) -> Result<Device, String> {
        let serial_number = device.serial_number().map_err(|e| e.to_string())?;
        let existing_assigned = devices.game_id_by_serial(serial_number);

        let assigned_game_id = match device.device_type() {
            DeviceType::WebBrowser => {
                if preferred_game_id != 0 {
                    preferred_game_id
                } else {
                    existing_assigned
                        .filter(|&id| id != 0)
                        .unwrap_or_else(|| game.default_game().id())
                }
            }
            DeviceType::Worker | _ => existing_assigned.unwrap_or(0),
        };

        device.set_game_id(assigned_game_id);
        devices
            .validate_and_register(device)
            .map_err(|e| e.to_string())
    }
}

impl ServerHandler for PacketHandler {
    async fn handle_packet(
        &mut self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        use net::ClientEvent::*;

        self.sequence_id = self.sequence_id.wrapping_add(1);

        match packet.event() {
            HandshakeHeartbeat if self.validated_device.is_some() => self.handle_heartbeat(),
            HandshakeRequest | HandshakeHeartbeat => self.handle_handshake(packet, address),
            event if self.validated_device.is_none() => self.handle_unvalidated(event, address),
            event if !self.can_control_game() => self.handle_unauthorized(event, address),
            GameSelect => self.handle_game_select(packet, address),
            GameCreate => self.handle_game_create(packet, address),
            GameDelete => self.handle_game_delete(packet, address),
            DeviceDisconnect => self.handle_device_disconnect(packet, address),
            DeviceAssignGame => self.handle_device_assign_game(packet),
            GameStart | GameResume => self.handle_game_resume(packet),
            GamePause => self.handle_game_pause(packet),
            GameEnd => self.handle_game_end(packet),
            GameReset => self.handle_game_reset(packet),
            GameScoreHomeFT => self.add_score(packet, Team::Home, Score::FreeThrow),
            GameScoreHome2FG => self.add_score(packet, Team::Home, Score::FieldGoal2),
            GameScoreHome3FG => self.add_score(packet, Team::Home, Score::FieldGoal3),
            GameScoreAwayFT => self.add_score(packet, Team::Away, Score::FreeThrow),
            GameScoreAway2FG => self.add_score(packet, Team::Away, Score::FieldGoal2),
            GameScoreAway3FG => self.add_score(packet, Team::Away, Score::FieldGoal3),
            _ => None,
        }
    }
}

impl PacketHandler {
    fn handle_heartbeat(&mut self) -> Option<ServerPacket> {
        Some(self.build_handshake_ack())
    }

    fn handle_unvalidated(
        &self,
        event: ClientEvent,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        tracing::warn!("[WS {address:?}] Unvalidated client attempted {event:?}");
        None
    }

    fn handle_unauthorized(
        &self,
        event: ClientEvent,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        tracing::warn!("[WS {address:?}] Unauthorized device attempted {event:?}");
        None
    }

    fn build_handshake_ack(&mut self) -> ServerPacket {
        let game = self
            .selected_game()
            .unwrap_or_else(|| Game::new_with_id(0, Discipline::Fiba5v5));
        ServerPacket::new(ServerEvent::HandshakeAck, self.sequence_id, game.to_bytes())
    }

    fn handle_handshake(
        &mut self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let device = match Device::try_from(packet) {
            Ok(device) => device,
            Err(error) => {
                tracing::warn!("[WS {address:?}] Handshake rejected (invalid payload: {error})");
                return None;
            }
        };

        let registered =
            match Self::register_device(&self.devices, &self.game, device, self.selected_game_id) {
                Ok(registered) => registered,
                Err(error) => {
                    tracing::warn!("[WS {address:?}] Device validation failed: {error}");
                    return None;
                }
            };

        self.selected_game_id = registered.game_id();
        if registered.device_type() == DeviceType::WebBrowser {
            tracing::info!(
                "[WS {address:?}] Validated client {:?} (id={}, sn={}, game_id={})",
                registered.device_type(),
                registered.device_id().unwrap_or("unknown"),
                registered.serial_number().unwrap_or("unknown"),
                registered.game_id(),
            );
        }
        self.validated_device = Some(registered);
        Some(self.build_handshake_ack())
    }

    fn handle_game_select(
        &mut self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let requested_game = Game::try_from(packet).ok()?;
        let found = self.game.by_id(requested_game.id())?;
        self.update_device_game(found.id());
        tracing::info!(
            "[WS {address:?}] Client switched to game id={}",
            self.selected_game_id
        );
        Some(ServerPacket::new(
            ServerEvent::GameUpdate,
            self.sequence_id,
            found.to_bytes(),
        ))
    }

    fn handle_game_create(
        &mut self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let discipline = Game::try_from(packet)
            .map(|game| game.discipline())
            .unwrap_or(Discipline::Fiba5v5);
        let created = match self.game.create_game(discipline) {
            Ok(created) => created,
            Err(error) => {
                tracing::error!("[WS {address:?}] Failed to create game: {error}");
                return None;
            }
        };
        self.update_device_game(created.id());
        tracing::info!(
            "[WS {address:?}] Client created and selected game id={}",
            self.selected_game_id
        );
        Some(ServerPacket::new(
            ServerEvent::GameUpdate,
            self.sequence_id,
            created.to_bytes(),
        ))
    }

    fn handle_game_delete(
        &mut self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let Ok(target) = Game::try_from(packet) else {
            return None;
        };
        let game_id = target.id();
        let fallback = match self.game.delete_game(game_id) {
            Ok(fallback) => fallback,
            Err(error) => {
                tracing::warn!("[WS {address:?}] Failed to delete game id={game_id}: {error}");
                return None;
            }
        };
        tracing::info!("[WS {address:?}] Client deleted game id={game_id}");
        if self.selected_game_id == game_id {
            self.update_device_game(fallback.id());
            return Some(ServerPacket::new(
                ServerEvent::GameUpdate,
                self.sequence_id,
                fallback.to_bytes(),
            ));
        }
        None
    }

    fn update_device_game(&mut self, game_id: u64) {
        self.selected_game_id = game_id;
        let Some(mut device) = self.validated_device else {
            return;
        };
        device.set_game_id(game_id);
        self.validated_device = Some(device);
        if let Err(error) = self.devices.validate_and_register(device) {
            tracing::warn!("Failed to update device game selection (game_id={game_id}): {error}");
        }
    }

    fn handle_device_disconnect(
        &self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let target_device = Device::try_from(packet).ok()?;
        let target_serial_number = target_device.serial_number().ok()?;
        tracing::info!(
            "[WS {address:?}] Requested disconnect for device sn={target_serial_number}"
        );
        self.devices.disconnect_by_serial(target_serial_number);
        None
    }

    fn handle_device_assign_game(&self, packet: ClientPacket) -> Option<ServerPacket> {
        let target_device = Device::try_from(packet).ok()?;
        let target_serial_number = target_device.serial_number().ok()?;
        let target_game_id = target_device.game_id();
        if target_game_id == 0 || self.game.by_id(target_game_id).is_some() {
            self.devices
                .assign_game_by_serial(target_serial_number, target_game_id);
        }
        None
    }

    fn target_game_id(&self, packet: ClientPacket) -> u64 {
        Game::try_from(packet)
            .map(|game| game.id())
            .ok()
            .filter(|&id| id != 0)
            .unwrap_or(self.selected_game_id)
    }

    fn handle_game_resume(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.game.resume_for(self.target_game_id(packet));
        None
    }

    fn handle_game_pause(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.game.pause_for(self.target_game_id(packet));
        None
    }

    fn handle_game_end(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.game.end_for(self.target_game_id(packet));
        None
    }

    fn handle_game_reset(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.game.reset_for(self.target_game_id(packet));
        None
    }

    fn add_score(&self, packet: ClientPacket, team: Team, score: Score) -> Option<ServerPacket> {
        self.game
            .add_score_for(self.target_game_id(packet), team, score);
        None
    }
}
