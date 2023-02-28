// Copyright © 2019 Intel Corporation
// Copyright 2023 Rivos, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

#![crate_type = "staticlib"]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod aml;
pub mod rsdp;
pub mod sdt;

fn generate_checksum(data: &[u8]) -> u8 {
    (255 - data.iter().fold(0u8, |acc, x| acc.wrapping_add(*x))).wrapping_add(1)
}
