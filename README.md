# Q-Light Control

This is a simple script to control [Q-Light ST Series USB Tower Lights](https://www.qlight.com/en/products/?qpcateid=9). Their provided SDK only works on Windows, but the protocol is simple enough.

## Requirements
This uses the Rust [Hidapi](https://docs.rs/hidapi/latest/hidapi/) Crate which only works on Mac, Windows and Illumos.

I'm making this for me and my friends, so maybe Windows will be added some day if they want that.

## Usage
The lights themselves don't have USB Serial Numbers so it's a pain in the butt to control multiple of them.

You can use `qlight list` to get the list of lights attached based on Pid/Vid.

To set the colors, use `qlight set`. The CLI help should be self explanitory.

## Limitations
Haven't implemented control over the sound buzzer yet. The library might eventually be published too.
