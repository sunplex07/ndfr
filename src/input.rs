use anyhow::{anyhow, Result};
use evdev::{Device, InputEventKind, Key, AbsoluteAxisType};
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Debug)]
pub enum TouchEvent {
    Down(f64),
    Up,
    Motion(f64),
}

#[derive(Debug)]
pub enum InputEvent {
    Touch(TouchEvent),
    KeyPressed(u16),
    KeyReleased(u16),
    FnKeyPressed,
    FnKeyReleased,
}

pub struct KeyboardFeatures {
    pub device: Device,
    pub has_physical_esc: bool,
}

pub fn find_keyboard_features() -> Result<KeyboardFeatures> {
    for i in 0..20 {
        let path = format!("/dev/input/event{}", i);
        if let Ok(device) = Device::open(&path) {
            if let Some(keys) = device.supported_keys() {
                if keys.contains(Key::KEY_FN) {
                    let has_physical_esc = keys.contains(Key::KEY_ESC);
                    println!("[keyboard] Found keyboard device at {}", path);
                    println!("[keyboard] Physical ESC key detected: {}", has_physical_esc);
                    return Ok(KeyboardFeatures { device, has_physical_esc });
                }
            }
        }
    }
    Err(anyhow!("Could not find a suitable keyboard device."))
}

pub fn start_keyboard_handler(tx: Sender<InputEvent>, mut device: Device) -> Result<()> {
    thread::spawn(move || loop {
        match device.fetch_events() {
            Ok(events) => {
                for ev in events {
                    if let InputEventKind::Key(key) = ev.kind() {
                        if key.code() == Key::KEY_FN.code() {
                            if ev.value() == 1 {
                                println!("[keyboard] Fn key pressed");
                                tx.send(InputEvent::FnKeyPressed).unwrap();
                            } else if ev.value() == 0 {
                                println!("[keyboard] Fn key released");
                                tx.send(InputEvent::FnKeyReleased).unwrap();
                            }
                        } else {
                            match ev.value() {
                                1 => {
                                    println!("[keyboard] Key pressed: {:?}", key);
                                    tx.send(InputEvent::KeyPressed(key.code())).unwrap();
                                }
                                0 => {
                                    println!("[keyboard] Key released: {:?}", key);
                                    tx.send(InputEvent::KeyReleased(key.code())).unwrap();
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[keyboard] Error fetching events: {}", e);
                break;
            }
        }
    });
    Ok(())
}

pub fn find_touch_device() -> Result<Device> {
    for i in 0..20 {
        let path = format!("/dev/input/event{}", i);
        if let Ok(device) = Device::open(&path) {
            if matches!(device.name(), Some("Apple Inc. Apple T1 Controller Touchpad") | Some("Apple Inc. Touch Bar Display Touchpad")) {
                println!("[touch] Found touch device at {}", path);
                return Ok(device);
            }
        }
    }
    Err(anyhow!("Could not find a suitable touch device."))
}

pub fn start_touch_handler(tx: Sender<InputEvent>) -> Result<()> {
    let mut device = find_touch_device()?;
    thread::spawn(move || {
        let mut events_packet = Vec::new();
        loop {
            match device.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        let is_syn_report = ev.kind() == InputEventKind::Synchronization(evdev::Synchronization::SYN_REPORT);
                        events_packet.push(ev);

                        if is_syn_report {
                            let mut x_coord = None;
                            let mut touch_event = None;

                            for packet_ev in &events_packet {
                                match packet_ev.kind() {
                                    InputEventKind::Key(key) if key.code() == Key::BTN_TOUCH.code() => {
                                        touch_event = Some(packet_ev.value());
                                    }
                                    InputEventKind::AbsAxis(axis) if axis == AbsoluteAxisType::ABS_X => {
                                        x_coord = Some(packet_ev.value());
                                    }
                                    _ => {}
                                }
                            }

                            if let Some(value) = touch_event {
                                if value == 1 {
                                    if let Some(x) = x_coord {
                                        println!("[touch] Touch down at x: {}", x);
                                        tx.send(InputEvent::Touch(TouchEvent::Down(x as f64))).unwrap();
                                    }
                                } else {
                                    println!("[touch] Touch up");
                                    tx.send(InputEvent::Touch(TouchEvent::Up)).unwrap();
                                }
                            } else if let Some(x) = x_coord {
                                println!("[touch] Touch motion at x: {}", x);
                                tx.send(InputEvent::Touch(TouchEvent::Motion(x as f64))).unwrap();
                            }

                            events_packet.clear();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[touch] Error fetching events: {}", e);
                    break;
                }
            }
        }
    });
    Ok(())
}
