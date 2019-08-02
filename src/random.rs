use libc;
use libcontainer::errors::*;
use std::os::unix::io::RawFd;
use std::fs;
use nix::fcntl::{self, OFlag};
use nix::sys::stat::Mode;
use nix::errno::Errno;


pub const RNGDEV: &'static str = "/dev/random";
pub const RNDADDTOENTCNT: libc::c_int = 0x40045201;
pub const RNDRESEEDRNG: libc::c_int = 0x5207;

pub fn reseed_rng(data: &[u8]) -> Result<()> {
	let len = data.len() as libc::c_long;
	fs::write(RNGDEV, data)?;

	let fd = fcntl::open(RNGDEV, OFlag::O_RDWR, Mode::from_bits_truncate(0o022))?;

	let ret = unsafe { libc::ioctl(fd, RNDADDTOENTCNT, &len as *const libc::c_long) };
	let _ = Errno::result(ret).map(drop)?;

	let ret = unsafe { libc::ioctl(fd, RNDRESEEDRNG, 0) };
	let _ = Errno::result(ret).map(drop)?;

	Ok(())
}
