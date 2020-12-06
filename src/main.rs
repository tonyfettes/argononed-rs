extern crate libc;
extern crate signal_hook;

use serde::Deserialize;
use std::thread;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use rppal::gpio::{Gpio, Trigger, Level};
use rppal::i2c::I2c;
use signal_hook::iterator::Signals;

#[derive(Deserialize)]
struct FanConfig {
    dynamic: bool,
    const_fan_speed: Option<u8>,
    step: Option<Vec<TempSpeedPair>>,
    delay_on_change: Option<u64>,
}

#[derive(Deserialize)]
struct TempSpeedPair {
    temperature: i16,
    fan_speed: u8,
}

#[derive(Debug)]
enum ConfigError {
    NoConstantSpeed,
    EmptyStepConfig,
}

impl std::error::Error for ConfigError {}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConfigError::NoConstantSpeed => write!(f, "No const_fan_speed given when dynamic fan speed is set to false"),
            ConfigError::EmptyStepConfig => write!(f, "Empty temperature-fanspeed step configuration"),
        }
    }
}

const FAN_ADDR: u16 = 0x1a;

fn shutdown_check(gpio_interface: Gpio, shutdown_pin_loc: u8) -> Result<(), Box<dyn std::error::Error>> {
    let mut signals = Signals::new(&[
        signal_hook::SIGTERM,
        signal_hook::SIGINT,
        signal_hook::SIGQUIT,
    ])?;
    let mut shutdown_pin = gpio_interface.get(shutdown_pin_loc)?.into_input_pulldown();
    static PULSE_TIME: AtomicU8 = AtomicU8::new(0);
    shutdown_pin.set_async_interrupt(Trigger::RisingEdge, |level| {
        match level {
            Level::Low => {},
            Level::High => { PULSE_TIME.fetch_add(1, Ordering::SeqCst); },
        };
    })?;

    'outer: loop {
        for signal in signals.pending() {
            match signal as libc::c_int {
                signal_hook::SIGTERM | signal_hook::SIGINT | signal_hook::SIGQUIT => {
                    break 'outer;
                },
                _ => unreachable!(),
            }
        };
        match PULSE_TIME.load(Ordering::SeqCst) {
            2 | 3 => { Command::new("systemctl reboot").spawn()?; },
            4 | 5 => { Command::new("systemctl poweroff").spawn()?; },
            _ => {},
        };
    };
    return Ok(());
}

fn load_config(filename: &str) -> Result<FanConfig, Box<dyn std::error::Error>> {
    let mut fanconfig: FanConfig = toml::from_str::<FanConfig>(&std::fs::read_to_string(filename)?[..])?;
    match fanconfig.step {
        Some(ref mut step) => { step.sort_by(|a, b| a.temperature.cmp(&b.temperature)); },
        None => {},
    };
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
    let mut signals = Signals::new(&[
        signal_hook::SIGTERM,
        signal_hook::SIGINT,
        signal_hook::SIGQUIT,
    ])?;
    let config = load_config("/etc/argononed.conf")?;
    match config.dynamic {
        false => {
            match config.const_fan_speed {
                Some(speed) => { i2c_interface.smbus_write_byte(0, speed)?; },
                None => { return Err(std::boxed::Box::new(ConfigError::NoConstantSpeed)); },
            }
        },
        true => {
            let delay: u64 = match config.delay_on_change {
                None => 30,
                Some(delay) => delay,
            };
            match config.step {
                None => { return Err(std::boxed::Box::new(ConfigError::EmptyStepConfig)); },
                Some(step_config) => {
                    if step_config.len() == 0 {
                        return Err(std::boxed::Box::new(ConfigError::EmptyStepConfig));
                    }
                    let mut curret_fan_speed: u8 = 0;
                    'outer: loop {
                        for signal in signals.pending() {
                            match signal as libc::c_int {
                                signal_hook::SIGTERM | signal_hook::SIGINT | signal_hook::SIGQUIT => {
                                    i2c_interface.smbus_write_byte(0, 0)?;
                                    break 'outer;
                                },
                                _ => unreachable!(),
                            }
                        };
                        let current_temperature = read_temperature()?;
                        let mut target_fan_speed: u8 = 0;
                        for temperature_step in step_config.iter() {
                            if current_temperature < (temperature_step.temperature as f32) {
                                target_fan_speed = temperature_step.fan_speed;
                                break;
                            }
                        }
                        if target_fan_speed < curret_fan_speed {
                            thread::sleep(std::time::Duration::from_secs(delay));
                        }
                        curret_fan_speed = target_fan_speed;
                        i2c_interface.smbus_write_byte(0, curret_fan_speed)?;
                        thread::sleep(std::time::Duration::from_secs(delay));
                    };
                },
            };
        },
    };
    return Ok(());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpio_interface = Gpio::new()?;
    let mut i2c_interface = I2c::new()?;
    i2c_interface.set_slave_address(FAN_ADDR)?;
    let shutdown_check_handler = thread::spawn(move || {
        shutdown_check(gpio_interface, 4).expect("Error monitoring the shutdown button");
    });
    let fan_check_handler = thread::spawn(move || {
        return fan_check(i2c_interface).expect("Error keeping the fan running");
    });
    shutdown_check_handler.join().unwrap();
    fan_check_handler.join().unwrap();
    return Ok(());
}
