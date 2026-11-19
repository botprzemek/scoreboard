use crate::LedOutputs;
use crate::bitplane::BitplaneBuffer;
use crate::output::OutputPin;

impl<P: OutputPin> LedOutputs<P> {
    #[inline(never)]
    pub fn render_bcm_frame_esp32s3_fast<const WIDTH: usize, const HALF_HEIGHT: usize>(
        &mut self,
        bitplanes: &BitplaneBuffer<WIDTH, HALF_HEIGHT>,
        brightness_percent: u8,
    ) {
        const BASE_CYCLES: u32 = 1;
        const GPIO_OUT_W1TS: *mut u32 = 0x6000_4008 as *mut u32;
        const GPIO_OUT_W1TC: *mut u32 = 0x6000_400C as *mut u32;
        const GPIO_OUT1_W1TS: *mut u32 = 0x6000_4014 as *mut u32;
        const GPIO_OUT1_W1TC: *mut u32 = 0x6000_4018 as *mut u32;

        const CLK_BIT: u32 = 1 << 2;
        const OE_BIT: u32 = 1 << 14;

        const RGB1_MASK: u32 = (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 10);
        const ROW1_MASK: u32 = (1 << 3) | (1 << 4) | (1 << 13) | (1 << 16);
        const LAT1_BIT: u32 = 1 << 15;

        let brightness_factor = u32::from(brightness_percent.min(100));

        for bit in 0..8 {
            let full_cycles = BASE_CYCLES << bit;
            let on_cycles = (full_cycles * brightness_factor) / 100;
            let off_cycles = full_cycles.saturating_sub(on_cycles);

            for row in 0..HALF_HEIGHT {
                unsafe { GPIO_OUT_W1TS.write_volatile(OE_BIT) };

                for &mask in bitplanes.row_slice(row, bit) {
                    let m = u32::from(mask);
                    let set_mask = ((m & 1) << 10)
                        | (((m >> 1) & 1) << 9)
                        | (((m >> 2) & 1) << 8)
                        | (((m >> 3) & 1) << 6)
                        | (((m >> 4) & 1) << 7)
                        | (((m >> 5) & 1) << 5);
                    let clr_mask = RGB1_MASK & !set_mask;

                    unsafe {
                        GPIO_OUT1_W1TC.write_volatile(clr_mask);
                        GPIO_OUT1_W1TS.write_volatile(set_mask);
                        Self::delay_cycles(2);
                        GPIO_OUT_W1TS.write_volatile(CLK_BIT);
                        Self::delay_cycles(4);
                        GPIO_OUT_W1TC.write_volatile(CLK_BIT);
                    }
                }

                let r = row as u8;
                let mut row_set = 0u32;
                if (r & 0x01) != 0 {
                    row_set |= 1 << 13;
                }
                if (r & 0x02) != 0 {
                    row_set |= 1 << 4;
                }
                if (r & 0x04) != 0 {
                    row_set |= 1 << 16;
                }
                if (r & 0x08) != 0 {
                    row_set |= 1 << 3;
                }
                let row_clr = ROW1_MASK & !row_set;

                unsafe {
                    GPIO_OUT1_W1TC.write_volatile(row_clr);
                    GPIO_OUT1_W1TS.write_volatile(row_set);
                    Self::delay_cycles(4);
                    GPIO_OUT1_W1TS.write_volatile(LAT1_BIT);
                    Self::delay_cycles(8);
                    GPIO_OUT1_W1TC.write_volatile(LAT1_BIT);
                    Self::delay_cycles(4);
                }

                if on_cycles > 0 {
                    unsafe { GPIO_OUT_W1TC.write_volatile(OE_BIT) };
                    Self::delay_cycles(on_cycles);
                    unsafe { GPIO_OUT_W1TS.write_volatile(OE_BIT) };
                }

                if off_cycles > 0 {
                    Self::delay_cycles(off_cycles);
                }
            }
        }

        unsafe {
            GPIO_OUT_W1TS.write_volatile(OE_BIT);
            GPIO_OUT1_W1TC.write_volatile(LAT1_BIT);
        }
    }

    #[inline(always)]
    fn delay_cycles(cycles: u32) {
        let dummy = 0u32;
        for _ in 0..cycles {
            unsafe {
                core::ptr::read_volatile(&dummy);
            }
        }
    }
}
