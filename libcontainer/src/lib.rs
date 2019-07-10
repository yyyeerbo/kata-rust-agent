#[macro_use]
extern crate error_chain;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate protocols;
extern crate caps;
#[macro_use]
extern crate scopeguard;
extern crate prctl;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate protobuf;
#[macro_use]
extern crate log;
#[macro_use]
extern crate scan_fmt;


pub mod errors;
pub mod container;
pub mod process;
pub mod cgroups;
pub mod mount;
pub mod specconv;
pub mod sync;

// pub mod factory;
//pub mod configs;
// pub mod devices;
// pub mod init;
// pub mod rootfs;
// pub mod capabilities;
// pub mod console;
pub mod stats;
// pub mod user;
//pub mod intelrdt;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
