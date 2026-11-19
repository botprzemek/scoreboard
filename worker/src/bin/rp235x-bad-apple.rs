#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

use scoreboard_worker::peripherals::{self, LedPeripherals};

use defmt_rtt as _;
use embassy_rp::multicore::{self, Stack};
use embassy_time::Instant;
use hub75::{DisplayBuffer, LedOutputs};
use panic_probe as _;
use static_cell::StaticCell;

const WIDTH: usize = 64;
const HEIGHT: usize = 32;
const HALF_HEIGHT: usize = HEIGHT / 2;
const FRAME_BYTES: usize = (WIDTH * HEIGHT) / 2;
const DISPLAY_BRIGHTNESS: u8 = 100;
const FRAME_DURATION_MICROS: u64 = 33_333;

const NEW_FRAME_BIT: u8 = 0x80;
const SLOT_MASK: u8 = 0x03;

static BAD_APPLE_STREAM: &[u8] = include_bytes!("../../assets/bad_apple.bin");

pub static mut CORE1_STACK: Stack<8192> = Stack::new();

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

#[cortex_m_rt::entry]
fn main() -> ! {
    let raw_peripherals = embassy_rp::init(Default::default());
    let board_peripherals = peripherals::split(raw_peripherals);

    let s0 = BUF0.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s1 = BUF1.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s2 = BUF2.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));

    for slot in [&*s0, &*s1, &*s2] {
        unsafe { (*slot.0.get()).set_brightness(DISPLAY_BRIGHTNESS) };
    }

    BUFFER_PTRS[0].store(ptr::from_ref(s0).cast_mut(), Ordering::Release);
    BUFFER_PTRS[1].store(ptr::from_ref(s1).cast_mut(), Ordering::Release);
    BUFFER_PTRS[2].store(ptr::from_ref(s2).cast_mut(), Ordering::Release);

    let mut decoder = StreamDecoder::new();
    let first = decoder.next_frame();
    unsafe { unpack_frame(&mut *s0.0.get(), first) };

    let core1_stack = unsafe { &mut *ptr::addr_of_mut!(CORE1_STACK) };
    multicore::spawn_core1(board_peripherals.core1, core1_stack, move || {
        core1_entry(board_peripherals.led);
    });

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

#[unsafe(link_section = ".data.ram_func")]
#[inline(never)]
fn core1_display_loop(outputs: &mut LedOutputs<embassy_rp::gpio::Output<'static>>) -> ! {
    let mut front: u8 = 0;

    loop {
        if (SHARED_SLOT.load(Ordering::Acquire) & NEW_FRAME_BIT) != 0 {
            front = SHARED_SLOT.swap(front, Ordering::AcqRel) & SLOT_MASK;
        }

        let slot_ptr = BUFFER_PTRS[(front & SLOT_MASK).min(2) as usize].load(Ordering::Relaxed);
        let bitplanes = unsafe { (*(*slot_ptr).0.get()).bitplanes() };
        outputs.render_bcm_frame_sio_fast(bitplanes, DISPLAY_BRIGHTNESS);
    }
}
