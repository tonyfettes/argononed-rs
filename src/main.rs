use serde::Deserialize;
use std::thread;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use rppal::gpio::{Gpio, Trigger, Level};
use rppal::i2c::I2c;

#[derive(Deserialize)]
struct FanConfig {
    config: Vec<TempSpeedPair>,
}

#[derive(Deserialize)]
struct TempSpeedPair(i16, u8);

#[derive(Debug)]
enum ConfigError {
    EmptyConfigError,
}

impl std::error::Error for ConfigError {}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConfigError::EmptyConfigError => write!(f, "Empty Config"),
        }
    }
}

const FAN_ADDR: u16 = 0x1a;

fn shutdown_check(gpio_interface: Gpio, shutdown_pin_loc: u8) -> Result<(), Box<dyn std::error::Error>> {
    let mut shutdown_pin = gpio_interface.get(shutdown_pin_loc)?.into_input_pulldown();
    static PULSE_TIME: AtomicU8 = AtomicU8::new(0);
    shutdown_pin.set_async_interrupt(Trigger::RisingEdge, |level| {
        match level {
            Level::Low => {},
            Level::High => { PULSE_TIME.fetch_add(1, Ordering::SeqCst); },
        };
    })?;

    loop {
        match PULSE_TIME.load(Ordering::SeqCst) {
            2 | 3 => { Command::new("systemctl reboot").spawn()?; },
            4 | 5 => { Command::new("systemctl poweroff").spawn()?; },
            _ => {},
        }
    };
}

fn load_config(filename: &str) -> Result<FanConfig, Box<dyn std::error::Error>> {
    let mut fanconfig: FanConfig = toml::from_str::<FanConfig>(&std::fs::read_to_string(filename)?[..])?;
    fanconfig.config.sort_by(|a, b| a.0.cmp(&b.0));
    return Ok(fanconfig);
}

fn read_temperature() -> Result<f32, Box<dyn std::error::Error>> {
    return Ok(std::str::from_utf8(&Command::new("/opt/vc/bin/vcgencmd")
            .arg("measure_temp")
            .output()?
            .stdout[..])?
        .trim_start_matches("temp=")
        .trim_end()
        .trim_end_matches("\'C")
        .parse::<f32>()?);
}

fn fan_check(i2c_interface: I2c) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config("/etc/argononed.conf")?;
    if config.config.len() == 0 {
        return Err(std::boxed::Box::new(ConfigError::EmptyConfigError));
    }
    let mut curret_fan_speed: u8 = 0;
    loop {
        let current_temperature = read_temperature()?;
        let mut target_fan_speed: u8 = 0;
        for temperature_step in config.config.iter() {
            if current_temperature < (temperature_step.0 as f32) {
                target_fan_speed = temperature_step.1;
                break;
            }
        }
        if target_fan_speed < curret_fan_speed {
            thread::sleep(std::time::Duration::from_secs(30));
        }
        curret_fan_speed = target_fan_speed;
        i2c_interface.smbus_write_byte(0, curret_fan_speed)?;
        thread::sleep(std::time::Duration::from_secs(30));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpio_interface = Gpio::new()?;
    let mut i2c_interface = I2c::new()?;
    i2c_interface.set_slave_address(FAN_ADDR)?;
    let shutdown_check_handler = thread::spawn(move || {
        shutdown_check(gpio_interface, 4).expect("Error monitoring the shutdown button");
    });
    let fan_check_handler = thread::spawn(move || {
        fan_check(i2c_interface).expect("Error keeping the fan running");
    });
    shutdown_check_handler.join().unwrap();
    fan_check_handler.join().unwrap();
    Ok(())
}
