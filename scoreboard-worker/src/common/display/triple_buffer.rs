use crate::common::display::layout::ScoreboardLayout;
use crate::common::traits::DisplayTarget;

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};
use scoreboard_basketball::Game;
use scoreboard_hub75::{CORAL_RED, Color, DisplayBuffer, HEIGHT, WIDTH};

const NEW_FRAME_BIT: u8 = 0x80;
const SLOT_MASK: u8 = 0x03;

/// Lock-free triple-buffered `DisplayBuffer` shared between Core 0 (producer) and Core 1 (consumer).
///
/// Core 0 renders layouts and computes bitplanes into the back slot and atomically publishes it.
/// Core 1 only swaps slot indices via atomics and streams bitplanes nonstop to HUB75 — zero flicker.
pub struct TripleDisplayBuffer {
    slots: [UnsafeCell<DisplayBuffer>; 3],
    shared_slot: AtomicU8,
    front_slot: AtomicU8,
    back_slot: AtomicU8,
}

// SAFETY: At any point in time, Core 0 exclusively owns `back_slot`, Core 1 exclusively owns
// `front_slot`, and `shared_slot` holds the third slot index. Handoffs use `AcqRel` swaps.
unsafe impl Sync for TripleDisplayBuffer {}

impl Default for TripleDisplayBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TripleDisplayBuffer {
    pub const fn new() -> Self {
        Self {
            slots: [
                UnsafeCell::new(DisplayBuffer::new()),
                UnsafeCell::new(DisplayBuffer::new()),
                UnsafeCell::new(DisplayBuffer::new()),
            ],
            shared_slot: AtomicU8::new(1),
            front_slot: AtomicU8::new(0),
            back_slot: AtomicU8::new(2),
        }
    }

    /// Producer (Core 0): render into the current back buffer and publish it atomically.
    pub fn write_back(&self, render: impl FnOnce(&mut DisplayBuffer)) {
        let back = self.back_slot.load(Ordering::Relaxed) & SLOT_MASK;
        // SAFETY: `back` is exclusively owned by the single producer (Core 0).
        let buf = unsafe { &mut *self.slots[usize::from(back.min(2))].get() };
        render(buf);

        let next_back = self
            .shared_slot
            .swap(back | NEW_FRAME_BIT, Ordering::AcqRel)
            & SLOT_MASK;
        self.back_slot.store(next_back, Ordering::Relaxed);
    }

    /// Consumer (Core 1): swap in the latest published frame if available, then return it.
    #[inline]
    pub fn front_buffer(&self) -> &DisplayBuffer {
        let mut front = self.front_slot.load(Ordering::Relaxed) & SLOT_MASK;
        if (self.shared_slot.load(Ordering::Acquire) & NEW_FRAME_BIT) != 0 {
            front = self.shared_slot.swap(front, Ordering::AcqRel) & SLOT_MASK;
            self.front_slot.store(front, Ordering::Relaxed);
        }

        // SAFETY: `front` is exclusively owned by the single consumer (Core 1).
        unsafe { &*self.slots[usize::from(front.min(2))].get() }
    }
}

pub static TRIPLE_DISPLAY: TripleDisplayBuffer = TripleDisplayBuffer::new();

/// Producer-side [`DisplayTarget`] that renders into a [`TripleDisplayBuffer`] back slot.
pub struct Hub75BackBuffer<'a> {
    triple: &'a TripleDisplayBuffer,
    layout: ScoreboardLayout,
    idle_animation: bool,
}

impl<'a> Hub75BackBuffer<'a> {
    pub const fn new(triple: &'a TripleDisplayBuffer, idle_animation: bool) -> Self {
        Self {
            triple,
            layout: ScoreboardLayout::new(),
            idle_animation,
        }
    }
}

impl DisplayTarget for Hub75BackBuffer<'_> {
    fn supports_idle_animation(&self) -> bool {
        self.idle_animation
    }

    fn update(&mut self, game: &Game, is_connected: bool) -> bool {
        let layout = &mut self.layout;
        self.triple.write_back(|buf| {
            buf.set_brightness(100);
            layout.render(buf, game, CORAL_RED, is_connected);
        });
        true
    }

    fn update_idle(&mut self, color: Color) {
        self.triple.write_back(|buf| {
            buf.set_brightness(100);
            buf.clear();
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    buf.set_pixel(x, y, color);
                }
            }
            buf.prepare_bitplanes();
        });
    }
}
