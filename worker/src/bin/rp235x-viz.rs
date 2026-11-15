#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

use scoreboard_worker::peripherals::{self, LedPeripherals};

use defmt_rtt as _;
use embassy_rp::multicore::{self, Stack};
use embassy_time::Instant;
use hub75::{Color, DisplayBuffer, LedOutputs};
use panic_probe as _;
use static_cell::StaticCell;

const WIDTH: usize = 64;
const HEIGHT: usize = 32;
const HALF_HEIGHT: usize = HEIGHT / 2;
const DISPLAY_BRIGHTNESS: u8 = 35;

const NEW_FRAME_BIT: u8 = 0x80;
const SLOT_MASK: u8 = 0x03;

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

/// Lock-free triple-buffer handoff between Core 0 (producer) and Core 1 (HUB75 scanner).
/// The three slot indices `{0, 1, 2}` remain pairwise distinct across `front`, `SHARED_SLOT & 3`, and `back`.
static SHARED_SLOT: AtomicU8 = AtomicU8::new(1);

static SIN_LUT: [i16; 256] = build_sin_lut();
static PALETTE_LUT: [Color; 256] = build_palette_lut();
static RAD_LUT: [[u8; WIDTH]; HEIGHT] = build_rad_lut();
static ANG_LUT: [[u8; WIDTH]; HEIGHT] = build_ang_lut();

#[cortex_m_rt::entry]
fn main() -> ! {
    let raw_peripherals = embassy_rp::init(Default::default());
    let board_peripherals = peripherals::split(raw_peripherals);

    let s0 = BUF0.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s1 = BUF1.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));
    let s2 = BUF2.init_with(|| BufferSlot(UnsafeCell::new(DisplayBuffer::new())));

    for slot in [&*s0, &*s1, &*s2] {
        unsafe { (*slot.0.get()).set_brightness(100) };
    }

    BUFFER_PTRS[0].store(ptr::from_ref(s0).cast_mut(), Ordering::Release);
    BUFFER_PTRS[1].store(ptr::from_ref(s1).cast_mut(), Ordering::Release);
    BUFFER_PTRS[2].store(ptr::from_ref(s2).cast_mut(), Ordering::Release);

    // Pre-render initial frame into slot 0 (Core 1's starting front buffer) before spawning Core 1.
    unsafe { render_visualizer(&mut *s0.0.get(), 0) };

    let core1_stack = unsafe { &mut *ptr::addr_of_mut!(CORE1_STACK) };
    multicore::spawn_core1(board_peripherals.core1, core1_stack, move || {
        core1_entry(board_peripherals.led);
    });

    let mut back: u8 = 2;
    loop {
        let micros = Instant::now().as_micros();
        let slot_ptr = BUFFER_PTRS[back as usize].load(Ordering::Acquire);
        unsafe { render_visualizer(&mut *(*slot_ptr).0.get(), micros) };

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

#[inline(always)]
fn sin_i32(phase: i32) -> i32 {
    i32::from(SIN_LUT[(phase as u8) as usize])
}

#[inline(always)]
fn cos_i32(phase: i32) -> i32 {
    i32::from(SIN_LUT[(phase.wrapping_add(64) as u8) as usize])
}

fn render_visualizer(display: &mut DisplayBuffer<WIDTH, HEIGHT, HALF_HEIGHT>, micros: u64) {
    // Sub-millisecond phase accumulators so fast motion stays fluid at high frame rates.
    let t_tunnel = ((micros / 4_000) as i32) & 255;
    let t_spin = ((micros / 5_500) as i32) & 255;
    let t_hue = ((micros / 11_000) as i32) & 255;
    let t_pulse = ((micros / 2_600) as i32) & 255;

    let t_w1 = ((micros / 2_800) as i32) & 255;
    let t_w2 = ((micros / 3_900) as i32) & 255;
    let t_w3 = ((micros / 1_900) as i32) & 255;

    // Layer 1: Swirling cyber vortex tunnel + plasma interference + beat shockwave ring.
    for y in 0..HEIGHT {
        let row_wave = sin_i32((y as i32) * 7 - t_spin) >> 2;
        for x in 0..WIDTH {
            let rad = i32::from(RAD_LUT[y][x]);
            let ang = i32::from(ANG_LUT[y][x]);

            let u = (rad + t_tunnel) & 255;
            let twist = sin_i32(u) >> 2;
            let v = (ang * 3 + twist - t_spin) & 255;

            let spiral = sin_i32(v + u) >> 1;
            let w1 = sin_i32(u * 2);
            let w2 = sin_i32(v + t_hue);
            let w3 = sin_i32((x as i32) * 5 + t_hue + row_wave);

            let idx = ((w1 + w2 + w3 + spiral) / 3 + 128 + t_hue) & 255;
            let [mut r, mut g, mut b] = PALETTE_LUT[idx as usize];

            // Keep backdrop balanced so foreground oscilloscope ribbons and sparks pop cleanly.
            r = ((u16::from(r) * 3) >> 2) as u8;
            g = ((u16::from(g) * 3) >> 2) as u8;
            b = ((u16::from(b) * 3) >> 2) as u8;

            let rdiff = (((rad * 2 - t_pulse + 128) & 255) - 128).abs();
            if rdiff < 20 {
                let glow = (20 - rdiff) * 6;
                r = r.saturating_add((glow / 3) as u8);
                g = g.saturating_add(glow as u8);
                b = b.saturating_add(glow as u8);
            }

            display.set_pixel(x, y, [r, g, b]);
        }
    }

    // Layer 2: Twin anti-aliased 2000s neon oscilloscope ribbons.
    for x in 0..WIDTH {
        let xi = x as i32;
        let wy_cyan = 248 + ((sin_i32(xi * 6 + t_w1) * 3 + sin_i32(xi * 3 - t_w2 * 2)) >> 2);
        let wy_magenta = 248 + ((sin_i32(xi * 9 - t_w2) * 3 + cos_i32(xi * 4 + t_w1)) >> 2);

        add_wave_column(display, x, wy_cyan, [70, 255, 255]);
        add_wave_column(display, x, wy_magenta, [255, 55, 225]);
    }

    // Layer 3: Three high-speed orbiting Lissajous energy comets with soft radial glow.
    let sparks: [(i32, i32, Color); 3] = [
        (
            31 * 16 + ((cos_i32(t_w3) * 26 * 16) >> 7),
            15 * 16 + ((sin_i32(t_w3 * 2 + 40) * 12 * 16) >> 7),
            [255, 255, 160],
        ),
        (
            31 * 16 + ((sin_i32(t_w1 * 2 + 90) * 27 * 16) >> 7),
            15 * 16 + ((cos_i32(t_w1 * 3) * 12 * 16) >> 7),
            [120, 255, 255],
        ),
        (
            31 * 16 + ((cos_i32(-t_w2 * 2 + 170) * 25 * 16) >> 7),
            15 * 16 + ((sin_i32(t_w2 * 3 + 20) * 11 * 16) >> 7),
            [255, 130, 255],
        ),
    ];

    for (sx16, sy16, color) in sparks {
        add_spark_glow(display, sx16, sy16, color);
    }

    display.prepare_bitplanes();
}

#[inline(always)]
fn add_wave_column(
    display: &mut DisplayBuffer<WIDTH, HEIGHT, HALF_HEIGHT>,
    x: usize,
    wy_sixteenths: i32,
    tint: Color,
) {
    let center_y = (wy_sixteenths / 16).clamp(0, (HEIGHT as i32) - 1);
    let y_start = (center_y - 1).max(0) as usize;
    let y_end = (center_y + 1).min((HEIGHT as i32) - 1) as usize;

    for y in y_start..=y_end {
        let dy = ((y as i32) * 16 - wy_sixteenths).abs();
        if dy < 24 {
            let strength = (24 - dy) as u16;
            let [r0, g0, b0] = display.frame_buffer()[y][x];
            let r = r0.saturating_add(((u16::from(tint[0]) * strength) / 20).min(255) as u8);
            let g = g0.saturating_add(((u16::from(tint[1]) * strength) / 20).min(255) as u8);
            let b = b0.saturating_add(((u16::from(tint[2]) * strength) / 20).min(255) as u8);
            display.set_pixel(x, y, [r, g, b]);
        }
    }
}

#[inline(always)]
fn add_spark_glow(
    display: &mut DisplayBuffer<WIDTH, HEIGHT, HALF_HEIGHT>,
    sx16: i32,
    sy16: i32,
    tint: Color,
) {
    let cx = sx16 / 16;
    let cy = sy16 / 16;
    let x_start = (cx - 2).max(0) as usize;
    let x_end = (cx + 2).min((WIDTH as i32) - 1) as usize;
    let y_start = (cy - 2).max(0) as usize;
    let y_end = (cy + 2).min((HEIGHT as i32) - 1) as usize;

    for y in y_start..=y_end {
        let dy = (y as i32) * 16 - sy16;
        for x in x_start..=x_end {
            let dx = (x as i32) * 16 - sx16;
            let dist2 = (dx * dx + dy * dy) >> 6;
            if dist2 < 16 {
                let strength = (16 - dist2) as u16;
                let [r0, g0, b0] = display.frame_buffer()[y][x];
                let r = r0.saturating_add(((u16::from(tint[0]) * strength) >> 4).min(255) as u8);
                let g = g0.saturating_add(((u16::from(tint[1]) * strength) >> 4).min(255) as u8);
                let b = b0.saturating_add(((u16::from(tint[2]) * strength) >> 4).min(255) as u8);
                display.set_pixel(x, y, [r, g, b]);
            }
        }
    }
}

const fn sin_approx_i16(phase: u8) -> i16 {
    if phase < 128 {
        let x = phase as i32;
        let p = x * (128 - x);
        ((4 * p * 127) / (20_480 - p)) as i16
    } else {
        let x = (phase - 128) as i32;
        let p = x * (128 - x);
        -(((4 * p * 127) / (20_480 - p)) as i16)
    }
}

const fn build_sin_lut() -> [i16; 256] {
    let mut lut = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        lut[i] = sin_approx_i16(i as u8);
        i += 1;
    }
    lut
}

const fn build_palette_lut() -> [Color; 256] {
    let mut lut = [[0, 0, 0]; 256];
    let mut i = 0;
    while i < 256 {
        let s_r = (sin_approx_i16((i as u8).wrapping_add(184)) as i32 + 127) as u32;
        let s_g = (sin_approx_i16((i as u8).wrapping_add(64)) as i32 + 127) as u32;
        let s_b = (sin_approx_i16((i as u8).wrapping_add(115)) as i32 + 127) as u32;
        let val = (128 - sin_approx_i16((i as u8).wrapping_add(64)) as i32) as u32;

        let r = (((s_r * s_r) / 254) * (35 + (220 * val) / 255)) / 255;
        let g = (((s_g * s_g) / 254) * (25 + (230 * val) / 255)) / 255;
        let b = (s_b * (65 + (190 * val) / 255)) / 255;

        lut[i] = [
            if r > 255 { 255 } else { r as u8 },
            if g > 255 { 255 } else { g as u8 },
            if b > 255 { 255 } else { b as u8 },
        ];
        i += 1;
    }
    lut
}

const fn isqrt_u32(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

const fn atan2_octant_u8(dy: i32, dx: i32) -> u8 {
    if dx == 0 && dy == 0 {
        return 0;
    }
    let ax = dx.abs();
    let ay = dy.abs();
    let (min_v, max_v) = if ax < ay { (ax, ay) } else { (ay, ax) };
    let base = (32 * min_v + (9 * min_v * (max_v - min_v)) / max_v) / max_v;
    let mut oct = if ax >= ay { base } else { 64 - base };
    if dx < 0 {
        oct = 128 - oct;
    }
    if dy < 0 {
        oct = 256 - oct;
    }
    (oct & 255) as u8
}

const fn build_rad_lut() -> [[u8; WIDTH]; HEIGHT] {
    let mut lut = [[0u8; WIDTH]; HEIGHT];
    let mut y = 0;
    while y < HEIGHT {
        let dy2 = (y as i32) * 2 - 31;
        let mut x = 0;
        while x < WIDTH {
            let dx2 = (x as i32) * 2 - 63;
            let dist16 = isqrt_u32(((dx2 * dx2 + dy2 * dy2) * 64) as u32);
            let inv = 2_880 / (dist16 + 40);
            lut[y][x] = (inv & 255) as u8;
            x += 1;
        }
        y += 1;
    }
    lut
}

const fn build_ang_lut() -> [[u8; WIDTH]; HEIGHT] {
    let mut lut = [[0u8; WIDTH]; HEIGHT];
    let mut y = 0;
    while y < HEIGHT {
        let dy = (y as i32) * 2 - 31;
        let mut x = 0;
        while x < WIDTH {
            let dx = (x as i32) * 2 - 63;
            lut[y][x] = atan2_octant_u8(dy, dx);
            x += 1;
        }
        y += 1;
    }
    lut
}
