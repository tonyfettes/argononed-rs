# Argonone

A Rust re-mplementation of argonone fan and power button monitor daemon.

# Install

Download it, compile it and put the compile file to a place that in your path

# Configure

Create a file at `/etc/argononed.conf` which is in toml file format and should be like this
```toml
config = [
  [30, 10],
  [40, 50],
  [50, 100],
]
```
