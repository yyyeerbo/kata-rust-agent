// Copyright (c) 2019 Ant Financial
//
// SPDX-License-Identifier: Apache-2.0
//

use serde;
use serde_json;
#[macro_use]
use serde_derive;

use std::io;

use crate::error::*;


static PROCERROR: &'static str = "procError";
static PROCREADY: &'static str  ="procReady";
static PROCRUN: &'statis str = "procRun";
static PROCHOOKS: &'static str = "procHooks";
static PROCRESUME: &'static str = "procResume";

#[derive(Serialize, Deserialize, Debug)]
pub struct SyncT {
#[serde(default, skip_serializing_if = "String::is_empty")]
	msg: String,
}

type SyncFn = fn(&SyncT) -> Result<()>;

pub fn write_sync(mut pipe: io::Write, sync: &str) -> Result<()>
{
	serde_json::to_writer(&mut pipe, &SyncT { msg: sync.to_string() })
}

pub fn read_sync(pipe: io::Read, expected: String) -> Result<()>
{
	let sync = serde_json::from_reader(pipe)?;
	if sync.msg == expected {
		Ok(())
	} else {
		Err(ErrorKind::ErrorCode("not equal").into())
	}
}

fpub fn parse_sync(pipe: io::Read, func: SyncFn) -> Result<()>
{
	let sync = serde_json::from_reader(pipe)?;
	func(&sync)
}
