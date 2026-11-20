use esp_hal::peripherals::{
    DMA_CH0, GPIO2, GPIO14, GPIO21, GPIO35, GPIO36, GPIO37, GPIO38, GPIO39, GPIO40, GPIO41, GPIO42,
    GPIO45, GPIO47, GPIO48, LCD_CAM,
};

#[non_exhaustive]
pub struct LedPeripherals {
    pub lcd_cam: LCD_CAM<'static>,
    pub dma_ch: DMA_CH0<'static>,

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
        lcd_cam: LCD_CAM<'static>,
        dma_ch: DMA_CH0<'static>,
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
            lcd_cam,
            dma_ch,
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
