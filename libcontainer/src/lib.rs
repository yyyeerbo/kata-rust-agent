extern crate error_chain;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate oci;
extern crate caps;


pub mod errors;
// pub mod factory;
pub mod container;
pub mod process;
pub mod configs;
pub mod devices;
pub mod cgroups;
pub mod init;
pub mod rootfs;
pub mod capabilities;
pub mod console;
pub mod stats;
pub mod user;
pub mod mount;
pub mod specconv;
pub mod intelrdt;
pub mod sync;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
