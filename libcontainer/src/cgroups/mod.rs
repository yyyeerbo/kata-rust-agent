use libcontainer::error::*;
use libcontainer::configs::{Stats, FreezerState, Config};


pub mod fs;
pub mod systemd;


pub trait Manager {
	fn apply(&self, pid: i32) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_pids(&self) -> Result<Vec<i32>> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_all_pids(&slef) -> Result<Vec<i32>> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_stats(&self) -> Result<Stats> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn freeze(&self, state: FreezerState) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn destroy(&self) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_paths(&self) -> Result<HashMap<String, String>> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn set(&self, container: Config) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}
}
