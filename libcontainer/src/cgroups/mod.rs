use crate::errors::*;
// use crate::configs::{FreezerState, Config};
use crate::stats::Stats;
use std::collections::HashMap;


pub mod fs;
pub mod systemd;

pub struct FreezerState {
}

pub struct Config {
}

pub trait Manager {
	fn apply(&self, pid: i32) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn get_pids(&self) -> Result<Vec<i32>> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn get_all_pids(&self) -> Result<Vec<i32>> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn get_stats(&self) -> Result<Stats> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn freeze(&self, state: FreezerState) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn destroy(&self) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn get_paths(&self) -> Result<HashMap<String, String>> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn set(&self, container: Config) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}
}
