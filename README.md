
# PulseAudio Monitor

This project contains a program I use to monitor the different sinks connected to my computer.
The daemon interfaces with PulseAudio directly and writes its results to certain files.
Then, scripts can be written to read and interpret these files.

## Daemon

The daemon is responsible for the following:
- Maintaining an always up-to-date reference to the newest sink
- Maintaining always up-to-date information on all sinks
- Notifying on sink change
- Switching sink-inputs on sink change

## Usage

- Compile the daemon
	```sh
	make
	```
- Run the daemon
	```sh
	pamonitor
	```
- Install the daemon (the default location is `$XDG_BIN_DIR`)
	```sh
	make install
	```

## Library Information

- [Crate](https://crates.io/crates/libpulse-binding)
- [Documentation](https://docs.rs/libpulse-binding/2.23.1/libpulse_binding/)

## TODO

