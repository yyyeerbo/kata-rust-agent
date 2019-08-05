// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use crate::errors::*;
// use crate::configs::{FreezerState, Config};
use std::collections::HashMap;
use protocols::oci::{Spec, LinuxResources};
use protocols::agent::{CgroupStats};


pub mod fs;
pub mod systemd;

pub type FreezerState = &'static str;

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

	fn get_stats(&self) -> Result<CgroupStats> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn freeze(&self, state: FreezerState) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn destroy(&mut self) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn get_paths(&self) -> Result<HashMap<String, String>> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}

	fn set(&self, container: &LinuxResources, update: bool) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!".to_string()).into())
	}
}
