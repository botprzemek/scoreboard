use crate::bitplane::BitplaneBuffer;
use crate::esp32s3::LedPeripherals;

use core::ptr::addr_of_mut;
use esp_hal::Blocking;
use esp_hal::dma::{DmaDescriptor, DmaTxBuf};
use esp_hal::gpio::interconnect::OutputSignal;
use esp_hal::lcd_cam::LcdCam;
use esp_hal::lcd_cam::lcd::i8080::{Command, Config, I8080};
use esp_hal::lcd_cam::lcd::{ClockMode, Phase, Polarity};
use esp_hal::time::Rate;

const LAT_BIT: u16 = 1 << 11;
/// Active-high bit on `DATA12`; `OE` pin is hardware-inverted in the GPIO matrix so
/// `DATA12 = 0` (including during `I8080` reset/idle) keeps physical `OE` HIGH (blanked).
const OE_ON_BIT: u16 = 1 << 12;
const BASE_WORDS: usize = 1;
const MAX_BIT_WORDS: usize = BASE_WORDS << 7;
const MAX_WORDS_PER_BITPLANE: usize = crate::HALF_HEIGHT * (crate::WIDTH + 3 + MAX_BIT_WORDS);
const DMA_BUF_BYTES: usize = MAX_WORDS_PER_BITPLANE * 2;
const DMA_DESC_COUNT: usize = DMA_BUF_BYTES.div_ceil(4092);

static mut DMA_DESCRIPTORS: [DmaDescriptor; DMA_DESC_COUNT] =
    [DmaDescriptor::EMPTY; DMA_DESC_COUNT];
static mut DMA_BUFFER: [u8; DMA_BUF_BYTES] = [0u8; DMA_BUF_BYTES];

/// Hardware-timed ESP32-S3 HUB75 driver backed by `LCD_CAM` I8080 16-bit parallel DMA.
pub struct Hub75Dma {
    i8080: Option<I8080<'static, Blocking>>,
    tx_buf: Option<DmaTxBuf>,
}

impl Hub75Dma {
    /// Initialize the `LCD_CAM` I8080 peripheral and GDMA channel on the HUB75 pins.
    #[must_use]
    pub fn new(peripherals: LedPeripherals) -> Self {
        let lcd_cam = LcdCam::new(peripherals.lcd_cam);
        let config = Config::default()
            .with_frequency(Rate::from_mhz(10))
            .with_clock_mode(ClockMode {
                polarity: Polarity::IdleLow,
                phase: Phase::ShiftLow,
            });

        let i8080 = I8080::new(lcd_cam.lcd, peripherals.dma_ch, config)
            .expect("valid LCD_CAM I8080 config")
            .with_wrx(peripherals.clk)
            .with_data0(peripherals.r1)
            .with_data1(peripherals.g1)
            .with_data2(peripherals.b1)
            .with_data3(peripherals.r2)
            .with_data4(peripherals.g2)
            .with_data5(peripherals.b2)
            .with_data6(peripherals.a)
            .with_data7(peripherals.b)
            .with_data8(peripherals.c)
            .with_data9(peripherals.d)
            .with_data10(peripherals.e)
            .with_data11(peripherals.lat)
            .with_data12(OutputSignal::from(peripherals.oe).with_output_inverter(true));

        // SAFETY: `Hub75Dma::new` consumes the singleton `LCD_CAM` and `DMA_CH0` peripherals,
        // so these static DMA buffers are only ever claimed once.
        let tx_buf = unsafe {
            DmaTxBuf::new(
                &mut *addr_of_mut!(DMA_DESCRIPTORS),
                &mut *addr_of_mut!(DMA_BUFFER),
            )
            .expect("valid HUB75 DMA TX buffer")
        };

        Self {
            i8080: Some(i8080),
            tx_buf: Some(tx_buf),
        }
    }

    /// Render one full 8-bit BCM frame via I8080 parallel DMA (one bitplane per transfer).
    #[inline(never)]
    pub fn render_bcm_frame_fast<const WIDTH: usize, const HALF_HEIGHT: usize>(
        &mut self,
        bitplanes: &BitplaneBuffer<WIDTH, HALF_HEIGHT>,
        brightness_percent: u8,
    ) {
        let brightness_factor = usize::from(brightness_percent.min(100));

        for bit in 0..8 {
            let full_words = BASE_WORDS << bit;
            let on_words = (full_words * brightness_factor) / 100;
            let off_words = full_words.saturating_sub(on_words);

            let mut tx_buf = self.tx_buf.take().expect("DMA TX buffer present");
            let i8080 = self.i8080.take().expect("I8080 peripheral present");

            let bytes_written = fill_bitplane_dma(
                tx_buf.as_mut_slice(),
                bitplanes,
                bit,
                on_words,
                off_words,
            );
            tx_buf.set_length(bytes_written);

            let transfer = i8080
                .send(Command::<u16>::None, 0, tx_buf)
                .expect("start HUB75 I8080 DMA transfer");
            let (_res, i8080, tx_buf) = transfer.wait();

            self.i8080 = Some(i8080);
            self.tx_buf = Some(tx_buf);
        }
    }
}

#[inline(always)]
fn push_word(buf: &mut [u8], pos: &mut usize, word: u16) {
    let bytes = word.to_le_bytes();
    buf[*pos] = bytes[0];
    buf[*pos + 1] = bytes[1];
    *pos += 2;
}

fn fill_bitplane_dma<const WIDTH: usize, const HALF_HEIGHT: usize>(
    buf: &mut [u8],
    bitplanes: &BitplaneBuffer<WIDTH, HALF_HEIGHT>,
    bit: usize,
    on_words: usize,
    off_words: usize,
) -> usize {
    let mut pos = 0usize;

    for row in 0..HALF_HEIGHT {
        let row_bits = (u16::from(row as u8) & 0x1F) << 6;

        // 1. Shift `WIDTH` pixels for this row with OE blanked (DATA12=0 -> physical OE HIGH).
        for &mask in bitplanes.row_slice(row, bit) {
            push_word(buf, &mut pos, u16::from(mask & 0x3F) | row_bits);
        }

        // 2. Latch shifted pixels while OE remains blanked (eliminates ghosting).
        push_word(buf, &mut pos, row_bits | LAT_BIT);
        push_word(buf, &mut pos, row_bits);

        // 3. Display BCM window: DATA12=1 (physical OE LOW) for `on_words`, then blanked for `off_words`.
        for _ in 0..on_words {
            push_word(buf, &mut pos, row_bits | OE_ON_BIT);
        }
        for _ in 0..off_words {
            push_word(buf, &mut pos, row_bits);
        }

        // 4. Trailing blank word so OE is guaranteed HIGH before next row / end of transfer.
        push_word(buf, &mut pos, row_bits);
    }

    pos
}
