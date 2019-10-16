#!/bin/bash
# Copyright (c) 2019 Ant Financial
#
# SPDX-License-Identifier: Apache-2.0
#

file ${TRAVIS_BUILD_DIR}/target/x86_64-unknown-linux-musl/release/kata-agent
for f in $(ls ${TRAVIS_BUILD_DIR}/target/x86_64-unknown-linux-musl/release/deps/kata_agent*); do
	file ${f}
done

sudo apt search musl
sudo apt search libstdc++-8-dev
