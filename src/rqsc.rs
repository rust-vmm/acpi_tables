// Copyright 2023 Rivos, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

use zerocopy::{byteorder, byteorder::LE, AsBytes};

extern crate alloc;
use alloc::vec::Vec;

use crate::{aml_as_bytes, assert_same_size, gas, Aml, AmlSink, Checksum, TableHeader};

type U16 = byteorder::U16<LE>;
type U32 = byteorder::U32<LE>;

#[repr(u8)]
pub enum ControllerType {
    Capacity = 0,
    Bandwidth = 1,
}

#[repr(u8)]
pub enum ResourceType {
    Cache = 0,
    Memory = 1,
}

pub struct RQSC {
    header: TableHeader,
    structures: Vec<QoSController>,
}

impl RQSC {
    pub fn new(oem_id: [u8; 6], oem_table_id: [u8; 8], oem_revision: u32) -> Self {
        let mut cksum = Checksum::default();

        let mut header = TableHeader {
            signature: *b"RQSC",
            length: (TableHeader::len() as u32).into(),
            revision: 1,
            checksum: 0,
            oem_id,
            oem_table_id,
            oem_revision: oem_revision.into(),
            creator_id: crate::CREATOR_ID,
            creator_revision: crate::CREATOR_REVISION,
        };
        cksum.append(header.as_bytes());
        header.checksum = cksum.value();

        Self {
            header,
            structures: Vec::new(),
        }
    }

    fn update_header(&mut self, data: &[u8]) {
        // Fix up the length of the table
        let len = data.len() as u32;
        let old_len = self.header.length.get();
        let new_len = len + old_len;
        self.header.length.set(new_len);

        // Fix up checksum
        self.header.checksum = 0;
        let mut cksum = Checksum::default();
        self.to_aml_bytes(&mut cksum);
        self.header.checksum = cksum.value();
    }

    pub fn add_controller(&mut self, q: QoSController) {
        self.structures.push(q);
        self.update_header(q.as_bytes());
    }
}

impl Aml for RQSC {
    fn to_aml_bytes(&self, sink: &mut dyn AmlSink) {
        for byte in self.header.as_bytes() {
            sink.byte(*byte);
        }

        sink.dword(self.structures.len() as u32);
        for st in &self.structures {
            st.to_aml_bytes(sink);
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default, AsBytes)]
pub struct QoSController {
    /// Identifies the specific register interface that is supported by this
    /// controller
    controller_type: u8,
    _reserved0: u8,
    length: U16,
    /// Register Buffer describing the starting address of the QoS register interface
    register: gas::GAS,
    _reserved1: [u8; 3],
    /// Describes the type of resource that this QoS controller has control over
    resource_type: u8,
    /// Depends on the Resource Type field. If Cache (0), this represents the unique Cache ID
    /// from the PPTT table's Cache Type structure (Table 5.159 in ACPI Spec 6.5) that this
    /// controller is associated with. If Memory, then this represents the proximity domain from
    /// the SRAT table that this specific controller is associated with. If SRAT is not
    /// implemented, then this shall be 0, indicating a UMA memory configuration.
    resource_id: U32,
    /// Non-zero number indicates that the controller supports allocation capability and the
    /// number of Resource Control IDs (RCID) supported by the controller. If 0, then no
    /// allocation control is available.
    rcid_count: U32,
    /// Non-zero number indicates that the controller supports usage monitoring capability and
    /// the number of Monitoring Control IDs (MCID) supported by the controller. If 0, then no
    /// usage monitoring is available.
    mcid_count: U32,
}

impl QoSController {
    pub fn new(
        controller_type: ControllerType,
        register_interface_address: gas::GAS,
        resource_type: ResourceType,
        resource_id: u32,
        rcid_count: u32,
        mcid_count: u32,
    ) -> Self {
        Self {
            controller_type: controller_type as u8,
            _reserved0: 0,
            length: 32u16.into(),
            register: register_interface_address,
            _reserved1: [0, 0, 0],
            resource_type: resource_type as u8,
            resource_id: resource_id.into(),
            rcid_count: rcid_count.into(),
            mcid_count: mcid_count.into(),
        }
    }
}

aml_as_bytes!(QoSController);
assert_same_size!(QoSController, [u8; 32]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas::*;

    #[test]
    fn test_bare_rqsc() {
        let rqsc = RQSC::new(*b"RQSSCC", *b"SOMETHIN", 0xcafe_d00d);
        let mut bytes = Vec::new();
        rqsc.to_aml_bytes(&mut bytes);
        let sum = bytes.iter().fold(0u8, |acc, x| acc.wrapping_add(*x));
        assert_eq!(sum, 0);
        assert_eq!(bytes.len(), TableHeader::len() + 4);
        assert_eq!(bytes[0..4], *b"RQSC");
    }

    #[test]
    fn test_structures() {
        let mut rqsc = RQSC::new(*b"RQSSCC", *b"SOMETHIN", 0xcafe_d00d);
        rqsc.add_controller(QoSController::new(
            ControllerType::Capacity,
            gas::GAS::new(
                AddressSpace::SystemMemory,
                64,
                0,
                AccessSize::QwordAccess,
                0x0123_4567_89ab_cdef,
            ),
            ResourceType::Memory,
            0x4242_4242,
            0x3737_3737,
            0x5656_5656,
        ));

        let mut bytes = Vec::new();
        rqsc.to_aml_bytes(&mut bytes);
        let sum = bytes.iter().fold(0u8, |acc, x| acc.wrapping_add(*x));

        assert_eq!(sum, 0);
        assert_eq!(
            bytes.len(),
            TableHeader::len() + 4 + core::mem::size_of::<QoSController>()
        );
        assert_eq!(bytes[0..4], *b"RQSC");
    }
}
