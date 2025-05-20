use anyhow::Result;
use anyhow::anyhow;
use esp_idf_svc::{
    hal::{
        gpio::{InterruptType, PinDriver, Pull},
        peripherals::Peripherals,
        task::notification::Notification,
    },
};
use rgb_led::{RGB8, WS2812RMT};
use std::num::NonZeroU32;
use getrandom::getrandom;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use heapless::String as HeapString;

use std::time::Duration;
use esp_idf_svc::sys::{
        esp_netif_get_handle_from_ifkey,
        esp_netif_get_ip_info,
        esp_netif_ip_info_t,
    };

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

const ST_SSID: &str = env!("ST_SSID");
const ST_PASS: &str = env!("ST_PASS");

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

    println!("Button setup ... DONE");


    println!(".....Booting up Wi-Fi AP + STA bridge........");
    let modem   = unsafe { Modem::new() };
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs     = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut ap_ssid = HeapString::<32>::new();
    ap_ssid.push_str(AP_SSID).expect("SSID too long");

    let mut ap_pass = HeapString::<64>::new();
    ap_pass.push_str(AP_PASS).expect("Password too long");

    let ap_cfg =  AccessPointConfiguration {
        ssid: ap_ssid,
        password: ap_pass,
        channel: 6,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };

    let mut st_ssid: HeapString<32> = HeapString::<32>::new();
    st_ssid.push_str(ST_SSID).expect("st_ssid too long");

    let mut st_pass: HeapString<64> = HeapString::<64>::new();
    st_pass.push_str(ST_PASS).expect("st_pass Password too long");

    println!(
        "Access point started! SSID: {}, password: {}",
        AP_SSID,
        AP_PASS
    );


    // ANCHOR: loop
    loop {
        // Enable interrupt and wait for new notificaton
        button.enable_interrupt()?;
        notification.wait(esp_idf_svc::hal::delay::BLOCK);
        println!("Button pressed!");
        let _ = random_light(&mut led);

        let sta_cfg = ClientConfiguration {
            ssid: st_ssid.clone(),
            password: st_pass.clone(),
            ..Default::default()
        };

        wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
        wifi.start()?;
        wifi.connect()?;
        println!("RustyAP up → SSID `{}`  pass `{}`", AP_SSID, AP_PASS);
        println!("Connecting STA to `{}` …", ST_SSID);

        // Wait until the STA has an IP lease
        while !wifi.is_connected()? {
            std::thread::sleep(Duration::from_millis(100));
        }

        // Enable NAT now
        enable_nat_on_sta()?;
        println!("NAPT active – devices under `{}` now have Internet", AP_SSID);
    }
    // ANCHOR_END: loop
}

#[allow(unused)]
// Generates random rgb values and sets them in the led.
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

extern "C" {
    fn ip_napt_enable(addr: u32, enable: i32);
}

#[allow(unused)]
pub fn enable_nat_on_sta() -> anyhow::Result<()> {
    // the default STA netif key is "WIFI_STA_DEF"
    let sta_netif = unsafe {
        esp_netif_get_handle_from_ifkey(b"WIFI_STA_DEF\0".as_ptr() as _)
    };
    if sta_netif.is_null() {
        return Err(anyhow!("STA netif not found"));
    }

    // query its IPv4 address
    let mut info = esp_netif_ip_info_t::default();
    unsafe { esp_netif_get_ip_info(sta_netif, &mut info) };

    // ip_napt_enable() expects the address in host byte-order
    let ip_host_order = u32::from_be(info.ip.addr);

    // unsafe { ip_napt_enable(ip_host_order, 1) };   // 1 = enable
    Ok(())
}