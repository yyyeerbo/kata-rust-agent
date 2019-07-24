use crate::netlink::{RtnlHandle, NETLINK_UEVENT};
use std::{thread};

pub const U_EVENT_ACTION: &'static str = "ACTION";
pub const U_EVENT_DEV_PATH: &'static str = "DEVPATH";
pub const U_EVENT_SUB_SYSTEM: &'static str = "SUBSYSTEM";
pub const U_EVENT_SEQ_NUM: &'static str = "SEQNUM";
pub const U_EVENT_DEV_NAME: &'static str = "DEVNAME";
pub const U_EVENT_INTERFACE: &'static str = "INTERFACE";

#[derive(Debug, Default)]
pub struct Uevent {
    action: String,
    devpath: String,
    devname: String,
    subsystem: String,
    seqnum: String,
    interface: String,
}

fn parse_uevent(message: &str) -> Uevent {
    let mut msg_iter = message.split('\0');
    let mut event = Uevent::default();

    msg_iter.next(); // skip the first value
    for arg in msg_iter {
        let key_val: Vec<&str> = arg.splitn(2, '=').collect();
        if key_val.len() == 2 {
            match key_val[0] {
                U_EVENT_ACTION => event.action = String::from(key_val[1]),
                U_EVENT_DEV_NAME => event.devname = String::from(key_val[1]),
                U_EVENT_SUB_SYSTEM => event.subsystem = String::from(key_val[1]),
                U_EVENT_DEV_PATH => event.devpath = String::from(key_val[1]),
                U_EVENT_SEQ_NUM => event.seqnum = String::from(key_val[1]),
                U_EVENT_INTERFACE => event.interface = String::from(key_val[1]),
                _  => error!("failed to match the event field"),
            }
        }
    }

    info!("got uevent message: {:?}", event);
    event
}

pub fn watch_uevents() {
    thread::spawn(move || {
        let rtnl = RtnlHandle::new(NETLINK_UEVENT, 1).unwrap();
        loop {
            match rtnl.recv_message() {
                Err(e) => error!("receive uevent message failed"),
                Ok(data) => {
                    let text = String::from_utf8(data);
                    match text {
                        Ok(text) => {
                            parse_uevent(&text);
                        },
                        Err(_) => error!("failed to convert bytes to text")
                    }
                }
            }
        }
    });
}
