# NDFR - Native Dynamic Function Row

NDFR is a WIP Touch Bar daemon designed to provide a native-like experience on MacBooks running Linux. 

![NDFR Default Layout](https://github.com/sunplex07/ndfr/blob/master/default.png?raw=true) 

## Requirements

System must have required input patches and driver for TouchBar.

### Core Dependencies
- **Rust:**  Install with RustUp
- **appletbdrm:** Display driver for the TouchBar.
- **`brightnessctl`:** Brightness Control.
- **`wpctl` (PipeWire) or `pactl` (PulseAudio):** Volume Control.

### Build Dependencies


**Arch Linux:**
```bash
sudo pacman -S base-devel cairo libinput libevdev libdrm
```

**Ubuntu/Debian:**
```bash
sudo apt-get install build-essential libcairo2-dev libinput-dev libevdev-dev libdrm-dev
```

## Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/sunplex07/ndfr.git
   cd ndfr
   ```

2. **Build the project:**
   ```bash
   cargo build --release
   ```
   The compiled binary will be located at `target/release/dfr_daemon`.

## Usage

NDFR requires direct access to hardware devices (`/dev/input/*` and `/dev/dri/*`), must be run with root privileges.

```bash
sudo ./target/release/dfr_daemon
```

### TODO

NOTE: This has only been tested on a T1 mac, as such there is no automatic detection for the Esc key, and the input/display device names may be different.

## Contributing

Contributions are always welcome. Feel free to open an issue or pull request for bugs, and fixes. 

## License

This project is licensed under the MIT License
