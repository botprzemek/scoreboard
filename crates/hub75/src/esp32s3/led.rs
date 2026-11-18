use crate::{LedOutputs, OutputPin};

use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{
    GPIO2, GPIO14, GPIO21, GPIO35, GPIO36, GPIO37, GPIO38, GPIO39, GPIO40, GPIO41, GPIO42, GPIO45,
    GPIO47, GPIO48,
};

#[non_exhaustive]
pub struct LedPeripherals {
    pub r1: GPIO42<'static>,
    pub g1: GPIO41<'static>,
    pub b1: GPIO40<'static>,
    pub r2: GPIO38<'static>,
    pub g2: GPIO39<'static>,
    pub b2: GPIO37<'static>,

    pub a: GPIO45<'static>,
    pub b: GPIO36<'static>,
    pub c: GPIO48<'static>,
    pub d: GPIO35<'static>,
    pub e: GPIO21<'static>,

    pub clk: GPIO2<'static>,
    pub lat: GPIO47<'static>,
    pub oe: GPIO14<'static>,
}

impl LedPeripherals {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        r1: GPIO42<'static>,
        g1: GPIO41<'static>,
        b1: GPIO40<'static>,
        r2: GPIO38<'static>,
        g2: GPIO39<'static>,
        b2: GPIO37<'static>,
        a: GPIO45<'static>,
        b: GPIO36<'static>,
        c: GPIO48<'static>,
        d: GPIO35<'static>,
        e: GPIO21<'static>,
        clk: GPIO2<'static>,
        lat: GPIO47<'static>,
        oe: GPIO14<'static>,
    ) -> Self {
        Self {
            r1,
            g1,
            b1,
            r2,
            g2,
            b2,
            a,
            b,
            c,
            d,
            e,
            clk,
            lat,
            oe,
        }
    }
}

impl OutputPin for Output<'_> {
    fn set_high(&mut self) {
        Self::set_high(self);
    }

    fn set_low(&mut self) {
        Self::set_low(self);
    }
}

impl LedOutputs<Output<'static>> {
    #[must_use]
    pub fn from_peripherals(peripherals: LedPeripherals) -> Self {
        Self::from(peripherals)
    }
}

impl From<LedPeripherals> for LedOutputs<Output<'static>> {
    fn from(peripherals: LedPeripherals) -> Self {
        let config = OutputConfig::default();

        Self {
            r1: Output::new(peripherals.r1, Level::Low, config),
            g1: Output::new(peripherals.g1, Level::Low, config),
            b1: Output::new(peripherals.b1, Level::Low, config),
            r2: Output::new(peripherals.r2, Level::Low, config),
            g2: Output::new(peripherals.g2, Level::Low, config),
            b2: Output::new(peripherals.b2, Level::Low, config),

            a: Output::new(peripherals.a, Level::Low, config),
            b: Output::new(peripherals.b, Level::Low, config),
            c: Output::new(peripherals.c, Level::Low, config),
            d: Output::new(peripherals.d, Level::Low, config),
            e: Output::new(peripherals.e, Level::Low, config),

            clk: Output::new(peripherals.clk, Level::Low, config),
            lat: Output::new(peripherals.lat, Level::Low, config),
            oe: Output::new(peripherals.oe, Level::Low, config),
        }
    }
}
