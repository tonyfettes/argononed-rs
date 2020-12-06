# Argonone

A Rust re-mplementation of argonone fan and power button monitor daemon.

## Install

Download it, compile it and put the compile file to a place that in your path.
Please have i2c feature of your Raspberry Pi on.

## Configure

Create a file at `/etc/argononed.conf` which is in toml file format and should be like this
```toml
# If set to true, please have 'step' set in your configuration file to use the
# feature to change fan speed according to temperature. If set to false, please
# have 'const_fan_speed' set in your configuration to set a constant fan speed.
dynamic = true
# As described above.
const_fan_speed = 0
# An array consists of pairs of temperature and fan_speed. When the detected
# temperature is greater than certain step but smaller than next step to that
# one, then that step will be used.
config = [
  { temperature = 40, fan_speed = 10  },
  { temperature = 50, fan_speed = 50  },
  { temperature = 60, fan_speed = 100 },
]
# The delay for the speed of the fan to change. Default to 30s if unset.
delay_on_change = 30
```

## Plan

- [ ] Have multiple config to set run mode for different time (like stop the fan
      at the night).
