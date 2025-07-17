# NDFR - Native Dynamic Function Row

NDFR is a WIP Touch Bar daemon designed to provide a native-like experience on MacBooks running Linux. 

![NDFR Default Layout](https://github.com/sunplex07/ndfr/blob/master/default.png?raw=true) 
![NDFR Now Playing](https://github.com/sunplex07/ndfr/blob/master/media-playing.png?raw=true) 

## Requirements

System must have required input patches and driver for TouchBar.

### Core Dependencies
- **Rust:**  Install with RustUp
- **appletbdrm:** Display driver for the TouchBar.
- **`brightnessctl`:** Brightness Control.
- **`wpctl` (PipeWire) or `pactl` (PulseAudio):** Volume Control.
- **`ndfr-media-helper`** - Bundled / Needed for scrubber functionality.

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
   cargo build && make
   ```
   The compiled binary will be located at `target/release/dfr_daemon`.
   
3. **Copy ndfr-media-helper**
   ```bash
   cp ./ndfr-media-helper /usr/bin/
   ```
   This is REQUIRED for media control.
   
4. **Copy other resources**
   ```bash
   cp -r ./icons ./target/debug/
   cp ./layout.yml ./target/debug/
   ```

5. **Run NDFR with Sudo**
   In one terminal:
   ```bash
   sudo ./target/debug/dfr_daemon
   ```
   Start the media relay in another. **Do not run this as sudo**
   ```bash
   chmod +x ./ndfr-media-agent.sh && ./ndfr-media-agent.sh
   ```


## Usage

NDFR requires direct access to hardware devices (`/dev/input/*` and `/dev/dri/*`), must be run with root privileges.

```bash
sudo ./target/release/dfr_daemon
```


### TODO

Futher testing on T1, T2, and Silicon macs.

Create an installer for building and copying resources.
Better handling of icons for media.

...?


## Contributing

Contributions are always welcome. Feel free to open an issue or pull request for bugs, and fixes. 

## License

This project is licensed under the MIT License
