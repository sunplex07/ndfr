use anyhow::Result;
use input_linux::uinput::UInputHandle;
use input_linux_sys::{input_id, uinput_setup, input_event, timeval};
use input_linux::{EventKind, Key};
use std::fs::{File, OpenOptions};

pub fn create_virtual_keyboard() -> Result<UInputHandle<File>> {
    let uinput_file = OpenOptions::new().write(true).open("/dev/uinput")?;
    let uinput = UInputHandle::new(uinput_file);

    uinput.set_evbit(EventKind::Key)?;
    for i in 1..248 {
        uinput.set_keybit(Key::from_code(i as u16)?)?;
    }

    let mut dev_name_c = [0i8; 80];
    let dev_name = "DFR Virtual Keyboard".as_bytes();
    for (i, byte) in dev_name.iter().enumerate() {
        dev_name_c[i] = *byte as i8;
    }

    uinput.dev_setup(&uinput_setup {
        id: input_id { bustype: 0x19, vendor: 0x1209, product: 0x316E, version: 1 },
        ff_effects_max: 0,
        name: dev_name_c,
    })?;
    uinput.dev_create()?;

    println!("[input] Virtual keyboard device created successfully.");
    Ok(uinput)
}

pub fn toggle_key(uinput: &mut UInputHandle<File>, key: Key, pressed: bool) -> Result<()> {
    let value = if pressed { 1 } else { 0 };
    let event = input_event {
        type_: EventKind::Key as u16,
        code: key as u16,
        value,
        time: timeval { tv_sec: 0, tv_usec: 0 },
    };
    uinput.write(&[event])?;
    let sync_event = input_event {
        type_: EventKind::Synchronize as u16,
        code: 0,
        value: 0,
        time: timeval { tv_sec: 0, tv_usec: 0 },
    };
    uinput.write(&[sync_event])?;
    Ok(())
}