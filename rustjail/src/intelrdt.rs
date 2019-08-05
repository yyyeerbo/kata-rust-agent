// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use crate::configs::{Config};
use crate::stats::Stats;
use crate::errors::*;
use std::sync::Mutex;

pub trait Manager {
	fn apply(&self, pid: i32) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_stats(&self) -> Result<Stats> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn destroy(&self) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn get_path(&self) -> Result<String> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}

	fn set(&self, config: &Config) -> Result<()> {
		Err(ErrorKind::ErrorCode("not supported!").into())
	}
}

pub struct IntelRdtManager<'a> {
	mutex: Mutex<i32>,
	config: &'a Config<'a>,
	id: String,
	path: String,
}

impl<'a> Manager for IntelRdtManager<'a> {
}
