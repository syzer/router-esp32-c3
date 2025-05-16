use anyhow::Result;
use anyhow::anyhow;
use esp_idf_svc::{
    hal::{
        gpio::{InterruptType, PinDriver, Pull},
        peripherals::Peripherals,
        task::notification::Notification,
    },
    sys::esp_random,
};
use rgb_led::{RGB8, WS2812RMT};
use std::num::NonZeroU32;
use getrandom::getrandom;
// use esp_idf_svc::hal::rmt::WS2812RMT;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();

    let peripherals = Peripherals::take()?;
    // ANCHOR: led
    let mut led = WS2812RMT::new(peripherals.pins.gpio2, peripherals.rmt.channel0)?;
    // ANCHOR_END: led

    // Configures the button
    let mut button = PinDriver::input(peripherals.pins.gpio9)?;
    button.set_pull(Pull::Up)?;
    button.set_interrupt_type(InterruptType::PosEdge)?;

    // Configures the notification
    let notification = Notification::new();
    let notifier = notification.notifier();

    // Subscribe and create the callback
    // Safety: make sure the `Notification` object is not dropped while the subscription is active
    unsafe {
        button.subscribe(move || {
            notifier.notify_and_yield(NonZeroU32::new(1).unwrap());
        })?;
    }

    println!("Hello world!");


    // ANCHOR: loop
    loop {
        // Enable interrupt and wait for new notificaton
        button.enable_interrupt()?;
        notification.wait(esp_idf_svc::hal::delay::BLOCK);
        println!("Button pressed!");
        // Generates random rgb values and sets them in the led.
        random_light(&mut led);
    }
    // ANCHOR_END: loop
}

#[allow(unused)]
pub fn random_light(led: &mut WS2812RMT) -> Result<()> {
    // Fill a 3-byte buffer with random data
    let mut buf = [0u8; 3];
    if let Err(e) = getrandom(&mut buf) {
        return Err(anyhow!("RNG failed: {:?}", e));
    }

    let colour = RGB8::new(buf[0], buf[1], buf[2]);

    // Push colour to the LED
    if let Err(e) = led.set_pixel(colour) {
        return Err(anyhow!("LED write error: {:?}", e));
    }

    Ok(())
}