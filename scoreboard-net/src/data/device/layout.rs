pub(crate) const DATA_SIZE: usize = crate::layout::DATA_SIZE;
pub(crate) const SERIAL_NUMBER_SIZE: usize = 12;

pub(crate) const DEVICE_ID_SIZE: usize = 12;
pub(crate) const DEVICE_TYPE: usize = 0x00;

pub(crate) const GAME_ID_SIZE: usize = core::mem::size_of::<u64>();
pub(crate) const GAME_ID_START: usize = 0x01;
pub(crate) const GAME_ID_END: usize = GAME_ID_START + GAME_ID_SIZE;

pub(crate) const SERIAL_NUMBER_START: usize = 0x09;
pub(crate) const SERIAL_NUMBER_END: usize = 0x15;

pub(crate) const DEVICE_ID_START: usize = 0x15;
pub(crate) const DEVICE_ID_END: usize = 0x21;
