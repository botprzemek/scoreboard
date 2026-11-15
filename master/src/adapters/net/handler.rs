use core::net::SocketAddr;
use std::sync::Arc;

use super::websocket::Session;
use crate::services::Services;

use basketball::{Discipline, Game, Score, Team};
use config::DeviceConfig;
use net::{
    ClientEvent, ClientPacket, Device, DeviceType, ServerEvent, ServerHandler, ServerPacket,
};

pub struct PacketHandler {
    services: Arc<Services>,
    sequence_id: u16,
    validated_device: Option<Device>,
    selected_game_id: u16,
}

impl PacketHandler {
    #[must_use]
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
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
    pub fn is_web_browser(&self) -> bool {
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
    pub fn selected_game_id(&self) -> u16 {
        self.selected_game_id
    }

    #[must_use]
    pub fn session(&self) -> Session {
        Session {
            validated: self.is_validated(),
            browser: self.is_web_browser(),
            game_id: self.selected_game_id,
        }
    }

    #[must_use]
    pub fn is_device(&self, serial_number: &str) -> bool {
        self.validated_device
            .is_some_and(|device| device.serial_number() == Ok(serial_number))
    }

    pub fn set_selected_game_id(&mut self, game_id: u16) {
        self.selected_game_id = game_id;
        if let Some(mut device) = self.validated_device {
            device.set_game_id(game_id);
            self.validated_device = Some(device);
        }
    }

    pub fn sync_assignment(&mut self) {
        if let Some(device) = self.validated_device
            && let Ok(serial_number) = device.serial_number()
            && let Some(assigned) = self.services.devices().game_id_by_serial(serial_number)
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
        self.services.game().by_id(self.selected_game_id)
    }

    pub fn register_device(
        services: &Services,
        mut device: Device,
        preferred_game_id: u16,
    ) -> Result<Device, String> {
        let serial_number = device.serial_number().map_err(|e| e.to_string())?;
        let existing_assigned = services.devices().game_id_by_serial(serial_number);

        let assigned_game_id = match device.device_type() {
            DeviceType::WebBrowser => {
                if preferred_game_id != 0 {
                    preferred_game_id
                } else {
                    existing_assigned
                        .filter(|&id| id != 0)
                        .unwrap_or_else(|| services.game().default_game().id())
                }
            }
            DeviceType::Worker | _ => existing_assigned.unwrap_or(0),
        };

        device.set_game_id(assigned_game_id);
        services
            .devices()
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
            GameSelect => self.handle_game_select(packet, address),
            GameCreate => self.handle_game_create(packet, address),
            DeviceDisconnect => self.handle_device_disconnect(packet, address),
            DeviceAssignGame => self.handle_device_assign_game(packet),
            GameStart | GameResume => self.handle_game_resume(packet).await,
            GamePause => self.handle_game_pause(packet).await,
            GameEnd => self.handle_game_end(packet).await,
            GameReset => self.handle_game_reset(packet).await,
            GameScoreHomeFT => self.add_score(packet, Team::Home, Score::FreeThrow).await,
            GameScoreHome2FG => self.add_score(packet, Team::Home, Score::FieldGoal2).await,
            GameScoreHome3FG => self.add_score(packet, Team::Home, Score::FieldGoal3).await,
            GameScoreAwayFT => self.add_score(packet, Team::Away, Score::FreeThrow).await,
            GameScoreAway2FG => self.add_score(packet, Team::Away, Score::FieldGoal2).await,
            GameScoreAway3FG => self.add_score(packet, Team::Away, Score::FieldGoal3).await,
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
        eprintln!("WARN: [WS {address:?}] Unvalidated client attempted {event:?}");
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
                eprintln!("WARN: [WS {address:?}] Handshake rejected (invalid payload: {error})");
                return None;
            }
        };

        let registered = match Self::register_device(&self.services, device, self.selected_game_id)
        {
            Ok(registered) => registered,
            Err(error) => {
                eprintln!("WARN: [WS {address:?}] Device validation failed: {error}");
                return None;
            }
        };

        self.selected_game_id = registered.game_id();
        if registered.device_type() == DeviceType::WebBrowser {
            println!(
                "INFO: [WS {address:?}] Validated client {:?} (id={}, sn={}, game_id={})",
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
        let found = self.services.game().by_id(requested_game.id())?;
        self.update_device_game(found.id());
        println!(
            "INFO: [WS {address:?}] Client switched to game id={}",
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
        let created = self.services.game().create_game(discipline);
        self.update_device_game(created.id());
        println!(
            "INFO: [WS {address:?}] Client created and selected game id={}",
            self.selected_game_id
        );
        Some(ServerPacket::new(
            ServerEvent::GameUpdate,
            self.sequence_id,
            created.to_bytes(),
        ))
    }

    fn update_device_game(&mut self, game_id: u16) {
        self.selected_game_id = game_id;
        let mut device = match self.validated_device {
            Some(device) => device,
            None => return,
        };
        device.set_game_id(game_id);
        self.validated_device = Some(device);
        let _ = self.services.devices().validate_and_register(device);
    }

    fn handle_device_disconnect(
        &self,
        packet: ClientPacket,
        address: Option<SocketAddr>,
    ) -> Option<ServerPacket> {
        let target_device = Device::try_from(packet).ok()?;
        let target_serial_number = target_device.serial_number().ok()?;
        println!(
            "INFO: [WS {address:?}] Requested disconnect for device sn={target_serial_number}"
        );
        self.services
            .devices()
            .disconnect_by_serial(target_serial_number);
        None
    }

    fn handle_device_assign_game(&self, packet: ClientPacket) -> Option<ServerPacket> {
        let target_device = Device::try_from(packet).ok()?;
        let target_serial_number = target_device.serial_number().ok()?;
        let target_game_id = target_device.game_id();
        if target_game_id == 0 || self.services.game().by_id(target_game_id).is_some() {
            self.services
                .devices()
                .assign_game_by_serial(target_serial_number, target_game_id);
        }
        None
    }

    fn target_game_id(&self, packet: ClientPacket) -> u16 {
        Game::try_from(packet)
            .map(|game| game.id())
            .ok()
            .filter(|&id| id != 0)
            .unwrap_or(self.selected_game_id)
    }

    async fn handle_game_resume(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.services
            .game()
            .resume_for(self.target_game_id(packet))
            .await;
        None
    }

    async fn handle_game_pause(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.services
            .game()
            .pause_for(self.target_game_id(packet))
            .await;
        None
    }

    async fn handle_game_end(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.services
            .game()
            .end_for(self.target_game_id(packet))
            .await;
        None
    }

    async fn handle_game_reset(&self, packet: ClientPacket) -> Option<ServerPacket> {
        self.services
            .game()
            .reset_for(self.target_game_id(packet))
            .await;
        None
    }

    async fn add_score(&self, packet: ClientPacket, team: Team, score: Score) -> Option<ServerPacket> {
        self.services
            .game()
            .add_score_for(self.target_game_id(packet), team, score)
            .await;
        None
    }
}
