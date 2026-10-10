#!/bin/sh
set -eu

cargo build --release
mkdir -p releases
cp target/release/maglife-cli releases/maglife-cli.bin
cp target/release/maglife-gtk releases/maglife-gtk.bin
chmod +x releases/maglife-cli.bin releases/maglife-gtk.bin
printf '%s\n' "Release binaries are in releases/"
