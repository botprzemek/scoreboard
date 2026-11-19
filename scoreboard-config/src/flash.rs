#[cfg(feature = "flash")]
use std::eprintln;
#[cfg(feature = "flash")]
use std::time::Duration;

#[cfg(any(feature = "flash", not(feature = "std")))]
use crate::{CONFIG_ADDR, CONFIG_SIZE, Config, ConfigError, Raw};

#[cfg(feature = "flash")]
use probe_rs::Permissions;
#[cfg(feature = "flash")]
use probe_rs::flashing::DownloadOptions;
#[cfg(feature = "flash")]
use probe_rs::probe::list::Lister;

#[cfg(any(feature = "flash", not(feature = "std")))]
pub trait FlashReader<'flash> {

    #[cfg(feature = "flash")]
    fn read() -> Result<Config, ConfigError>;

    #[cfg(not(feature = "std"))]
    fn read() -> Result<&'flash Config, ConfigError>;
}

#[cfg(any(feature = "flash", not(feature = "std")))]
pub trait FlashWriter {
    #[cfg(feature = "flash")]
    fn write(self) -> Result<(), ConfigError>;
}

#[cfg(any(feature = "flash", not(feature = "std")))]
impl<'flash> FlashReader<'flash> for Config {
    #[cfg(feature = "flash")]
    fn read() -> Result<Self, ConfigError> {
        use probe_rs::MemoryInterface;

        let mut buffer = [0u8; CONFIG_SIZE];
        let lister = Lister::new();
        let probes = lister.list_all();

        let first_probe = probes.first().ok_or(ConfigError::FileNotFound)?;

        let mut probe = lister.open(first_probe).map_err(|error| {
            eprintln!("failed to open debug probe: {error:?}");
            ConfigError::FileNotFound
        })?;

        let _ = probe.set_speed(5000);

        let mut session = probe
            .attach("RP2350", Permissions::default())
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;
        let mut core = session
            .core(0)
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;
        let address = u64::try_from(CONFIG_ADDR).map_err(|_| ConfigError::Serialization)?;
        core.read_8(address, &mut buffer)
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;
        let raw = Config::<Raw>::from_bytes(&buffer)?;

        raw.verify().cloned()
    }

    #[cfg(not(feature = "std"))]
    fn read() -> Result<&'flash Self, ConfigError> {
        unsafe {
            let bytes = core::slice::from_raw_parts(CONFIG_ADDR as *const u8, CONFIG_SIZE);
            let raw = Config::<Raw>::from_bytes(bytes)?;

            raw.verify()
        }
    }
}

#[cfg(feature = "flash")]
impl FlashWriter for Config {
    fn write(self) -> Result<(), ConfigError> {
        let lister = Lister::new();
        let probes = lister.list_all();

        let first_probe = probes.first().ok_or(ConfigError::FileNotFound)?;

        let mut probe = lister.open(first_probe).map_err(|error| {
            eprintln!("failed to open debug probe: {error:?}");
            ConfigError::FileNotFound
        })?;

        let _ = probe.set_speed(5000);

        let mut session = probe
            .attach("RP2350", Permissions::default())
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;

        let mut loader = session.target().flash_loader();
        let address = u64::try_from(CONFIG_ADDR).map_err(|_| ConfigError::Serialization)?;
        loader
            .add_data(address, self.as_bytes())
            .map_err(|_| ConfigError::Serialization)?;
        loader
            .commit(&mut session, DownloadOptions::default())
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;

        let mut core = session
            .core(0)
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;
        core.reset_and_halt(Duration::from_millis(500))
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;
        core.run()
            .map_err(|_| ConfigError::Io(std::io::ErrorKind::Other))?;

        Ok(())
    }
}
