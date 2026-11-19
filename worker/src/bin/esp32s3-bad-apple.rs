#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use core::ptr::{self, addr_of_mut};
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_time::Instant;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::system::{CpuControl, Stack};
use esp_hal::timer::timg::TimerGroup;
use hub75::{DisplayBuffer, LedOutputs, LedPeripherals};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const WIDTH: usize = 64;
const HEIGHT: usize = 32;
const HALF_HEIGHT: usize = HEIGHT / 2;
const FRAME_BYTES: usize = (WIDTH * HEIGHT) / 2;
const DISPLAY_BRIGHTNESS: u8 = 100;
const FRAME_DURATION_MICROS: u64 = 33_333;

const NEW_FRAME_BIT: u8 = 0x80;
const SLOT_MASK: u8 = 0x03;

static BAD_APPLE_STREAM: &[u8] = include_bytes!("../../assets/bad_apple.bin");

static mut APP_CORE_STACK: Stack<8192> = Stack::new();

struct BufferSlot(UnsafeCell<DisplayBuffer<WIDTH, HEIGHT, HALF_HEIGHT>>);
unsafe impl Sync for BufferSlot {}

static BUF0: StaticCell<BufferSlot> = StaticCell::new();
static BUF1: StaticCell<BufferSlot> = StaticCell::new();
static BUF2: StaticCell<BufferSlot> = StaticCell::new();

static BUFFER_PTRS: [AtomicPtr<BufferSlot>; 3] = [
    AtomicPtr::new(ptr::null_mut()),
    AtomicPtr::new(ptr::null_mut()),
    AtomicPtr::new(ptr::null_mut()),
];

static SHARED_SLOT: AtomicU8 = AtomicU8::new(1);

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

struct StreamDecoder {
    pos: usize,
    frame: [u8; FRAME_BYTES],
}

impl StreamDecoder {
    const fn new() -> Self {
        Self {
            pos: 0,
            frame: [0u8; FRAME_BYTES],
        }
    }

    fn next_frame(&mut self) -> &[u8; FRAME_BYTES] {
        if self.pos >= BAD_APPLE_STREAM.len() {
            self.pos = 0;
            self.frame.fill(0);
        }

        let mut cursor = 0usize;
        while cursor < FRAME_BYTES {
            let cmd = BAD_APPLE_STREAM[self.pos];
            self.pos += 1;

            if cmd < 128 {
                cursor += usize::from(cmd) + 1;
            } else if cmd < 192 {
                let count = usize::from(cmd - 127);
                let end = (cursor + count).min(FRAME_BYTES);
                while cursor < end {
                    self.frame[cursor] ^= BAD_APPLE_STREAM[self.pos];
                    self.pos += 1;
                    cursor += 1;
                }
            } else {
                let count = usize::from(cmd - 189);
                let val = BAD_APPLE_STREAM[self.pos];
                self.pos += 1;
                let end = (cursor + count).min(FRAME_BYTES);
                while cursor < end {
                    self.frame[cursor] ^= val;
                    cursor += 1;
                }
            }
        }

        &self.frame
    }
}

fn unpack_frame(
    display: &mut DisplayBuffer<WIDTH, HEIGHT, HALF_HEIGHT>,
    packed: &[u8; FRAME_BYTES],
) {
    for y in 0..HEIGHT {
        let row_base = y * (WIDTH / 2);
        for pair in 0..(WIDTH / 2) {
            let byte = packed[row_base + pair];
            let luma_left = (byte & 0x0F) * 17;
            let luma_right = (byte >> 4) * 17;
            let x = pair * 2;
            display.set_pixel(x, y, [luma_left, luma_left, luma_left]);
            display.set_pixel(x + 1, y, [luma_right, luma_right, luma_right]);
        }
    }
    display.prepare_bitplanes();
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    log::info!("=== ESP32-S3 Bad Apple starting ===");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let s0 = BUF0.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s1 = BUF1.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s2 = BUF2.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));

    for slot in [&*s0, &*s1, &*s2] {
        unsafe { (*slot.0.get()).set_brightness(100) };
    }

    BUFFER_PTRS[0].store(ptr::from_ref(s0).cast_mut(), Ordering::Release);
    BUFFER_PTRS[1].store(ptr::from_ref(s1).cast_mut(), Ordering::Release);
    BUFFER_PTRS[2].store(ptr::from_ref(s2).cast_mut(), Ordering::Release);

    let mut decoder = StreamDecoder::new();
    let first = decoder.next_frame();
    unsafe { unpack_frame(&mut *s0.0.get(), first) };

    let led_peripherals = LedPeripherals::new(
        peripherals.GPIO42,
        peripherals.GPIO41,
        peripherals.GPIO40,
        peripherals.GPIO38,
        peripherals.GPIO39,
        peripherals.GPIO37,
        peripherals.GPIO45,
        peripherals.GPIO36,
        peripherals.GPIO48,
        peripherals.GPIO35,
        peripherals.GPIO21,
        peripherals.GPIO2,
        peripherals.GPIO47,
        peripherals.GPIO14,
    );

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let _app_core_guard = unsafe {
        cpu_control
            .start_app_core(&mut *addr_of_mut!(APP_CORE_STACK), move || {
                core1_entry(led_peripherals)
            })
            .expect("failed to start Core 1")
    };

    let mut back: u8 = 2;
    let mut next_frame_at = Instant::now().as_micros() + FRAME_DURATION_MICROS;

    loop {
        let now = Instant::now().as_micros();
        if now < next_frame_at {
            core::hint::spin_loop();
            continue;
        }

        if now - next_frame_at > FRAME_DURATION_MICROS * 2 {
            next_frame_at = now + FRAME_DURATION_MICROS;
        } else {
            next_frame_at += FRAME_DURATION_MICROS;
        }

        let packed = decoder.next_frame();
        let slot_ptr = BUFFER_PTRS[back as usize].load(Ordering::Acquire);
        unsafe { unpack_frame(&mut *(*slot_ptr).0.get(), packed) };

        back = SHARED_SLOT.swap(back | NEW_FRAME_BIT, Ordering::AcqRel) & SLOT_MASK;
    }
}

fn core1_entry(led: LedPeripherals) -> ! {
    let mut outputs = LedOutputs::from_peripherals(led);
    core1_display_loop(&mut outputs)
}

#[inline(never)]
fn core1_display_loop(outputs: &mut LedOutputs<esp_hal::gpio::Output<'static>>) -> ! {
    let mut front: u8 = 0;

    loop {
        if (SHARED_SLOT.load(Ordering::Acquire) & NEW_FRAME_BIT) != 0 {
            front = SHARED_SLOT.swap(front, Ordering::AcqRel) & SLOT_MASK;
        }

        let slot_ptr = BUFFER_PTRS[(front & SLOT_MASK).min(2) as usize].load(Ordering::Relaxed);
        let bitplanes = unsafe { (*(*slot_ptr).0.get()).bitplanes() };
        outputs.render_bcm_frame_esp32s3_fast(bitplanes, DISPLAY_BRIGHTNESS);
    }
}
