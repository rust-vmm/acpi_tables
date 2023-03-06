// Copyright 2023 Rivos, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

use zerocopy::{byteorder, byteorder::LE, AsBytes};

extern crate alloc;
use alloc::{boxed::Box, vec::Vec};

use crate::{aml_as_bytes, assert_same_size, Aml, AmlSink, Checksum, TableHeader};

type U16 = byteorder::U16<LE>;
type U32 = byteorder::U32<LE>;
type U64 = byteorder::U64<LE>;

const RISCV_INTC_STRUCTURE: u8 = 0x18;
const RISCV_IMSIC_STRUCTURE: u8 = 0x19;
const RISCV_APLIC_STRUCTURE: u8 = 0x1a;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default, AsBytes)]
struct Header {
    pub table_header: TableHeader,
    /// Must be ignored by OSPM for RISC-V
    pub local_interrupt_controller_address: U32,
    pub flags: U32,
}

impl Header {
    fn len() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub struct MADT {
    header: Header,
    checksum: Checksum,
    structures: Vec<Box<dyn Aml>>,
    has_imsic: bool,
}

#[derive(Clone, Copy)]
pub enum LocalInterruptController {
    Riscv,
    Address(u32),
}

impl MADT {
    pub fn new(
        oem_id: [u8; 6],
        oem_table_id: [u8; 8],
        oem_revision: u32,
        int: LocalInterruptController,
    ) -> Self {
        let mut header = Header {
            table_header: TableHeader {
                signature: *b"APIC",
                length: (Header::len() as u32).into(),
                revision: 1,
                checksum: 0,
                oem_id,
                oem_table_id,
                oem_revision: oem_revision.into(),
                creator_id: crate::CREATOR_ID,
                creator_revision: crate::CREATOR_REVISION,
            },
            local_interrupt_controller_address: match int {
                LocalInterruptController::Riscv => 0,
                LocalInterruptController::Address(addr) => addr,
            }
            .into(),
            flags: 0.into(),
        };

        let mut cksum = Checksum::default();
        cksum.append(header.as_bytes());
        header.table_header.checksum = cksum.value();
        Self {
            header,
            checksum: cksum,
            structures: Vec::new(),
            has_imsic: false,
        }
    }

    fn update_header(&mut self, data: &[u8]) {
        let len = data.len() as u32;
        let old_len = self.header.table_header.length.get();
        let new_len = len + old_len;
        self.header.table_header.length.set(new_len);

        // Remove the bytes from the old length, add the new length
        // and the new data.
        self.checksum.delete(old_len.as_bytes());
        self.checksum.append(new_len.as_bytes());
        self.checksum.append(data);
        self.header.table_header.checksum = self.checksum.value();
    }

    pub fn add_rintc(&mut self, rintc: RINTC) {
        self.update_header(rintc.as_bytes());
        self.structures.push(Box::new(rintc));
    }

    pub fn add_imsic(&mut self, imsic: IMSIC) {
        assert!(!self.has_imsic);
        self.update_header(imsic.as_bytes());
        self.structures.push(Box::new(imsic));
        self.has_imsic = true;
    }

    pub fn add_aplic(&mut self, aplic: APLIC) {
        self.update_header(aplic.as_bytes());
        self.structures.push(Box::new(aplic));
    }
}

impl Aml for MADT {
    fn to_aml_bytes(&self, sink: &mut dyn AmlSink) {
        for byte in self.header.as_bytes() {
            sink.byte(*byte);
        }

        for st in &self.structures {
            st.to_aml_bytes(sink);
        }
    }
}

/// RISC-V Interrupt Controller (RINTC) structure
/// RISC-V platforms need to have a simple, per-hart interrupt controller
/// available to supervisor mode.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default, AsBytes)]
pub struct RINTC {
    pub r#type: u8,
    pub length: u8,
    pub version: u8,
    _reserved: u8,
    pub flags: U32,
    pub hart_id: U64,
    pub acpi_processor_uid: U32,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum HartStatus {
    Disabled = 0,
    Enabled = 1,
    OnlineCapable = 2,
}

impl RINTC {
    pub fn new(hart_status: HartStatus, mhartid: u64, acpi_processor_uid: u32) -> Self {
        Self {
            r#type: RISCV_INTC_STRUCTURE,
            length: RINTC::len() as u8,
            version: 1,
            _reserved: 0,
            flags: (hart_status as u32).into(),
            hart_id: mhartid.into(),
            acpi_processor_uid: acpi_processor_uid.into(),
        }
    }

    pub fn len() -> usize {
        core::mem::size_of::<Self>()
    }
}

assert_same_size!(RINTC, [u8; 0x14]);
aml_as_bytes!(RINTC);

// Even though IMSIC is a per-processor device, there should be only
// one IMSIC structure present in the MADT for a RISC-V system that
// provides information common across processors. The per-processor
// information will be provided by the RINTC structure.
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default, AsBytes)]
pub struct IMSIC {
    r#type: u8,
    length: u8,
    version: u8,
    _reserved: [u8; 5],
    // How many interrupt identities are supported by the IMSIC
    // interrupt file in supervisor mode (minimum 63 maximum 2047).
    num_supervisor_interrupt_identities: U16,
    // How many interrupt identities are supported by the IMSIC
    // interrupt file in guest mode (minimum 63 maximum 2047).
    num_guest_interrupt_identities: U16,
    // Number of guest index bits in MSI target address (0 - 7)
    guest_index_bits: u8,
    // Number of hart index bits in the MSI target address (0 - 15)
    hart_index_bits: u8,
    // Number of group index bits in the MSI target address (0 - 7)
    group_index_bits: u8,
    // LSB of the group index bits in the MSI target address (0 - 55)
    group_index_shift: u8,
}

impl IMSIC {
    pub fn new(
        num_supervisor_interrupt_identities: u16,
        num_guest_interrupt_identities: u16,
        guest_index_bits: u8,
        hart_index_bits: u8,
        group_index_bits: u8,
        group_index_shift: u8,
    ) -> Self {
        Self {
            r#type: RISCV_IMSIC_STRUCTURE,
            length: IMSIC::len() as u8,
            version: 1,
            _reserved: [0, 0, 0, 0, 0],
            num_supervisor_interrupt_identities: num_supervisor_interrupt_identities.into(),
            num_guest_interrupt_identities: num_guest_interrupt_identities.into(),
            guest_index_bits,
            hart_index_bits,
            group_index_bits,
            group_index_shift,
        }
    }

    pub fn len() -> usize {
        core::mem::size_of::<Self>()
    }
}

assert_same_size!(IMSIC, [u8; 16]);
aml_as_bytes!(IMSIC);

// The RISC-V AIA defines an APLIC for handling wired interrupts on a
// RISC-V platform. In a machine without IMSICs, every RISC-V hart
// accepts interrupts from exactly one APLIC which is the external
// interrupt controller for that hart. RISC-V harts that have IMSICs
// as their external interrupt controllers can receive external
// interrupts only in the form of MSIs. In that case, the role of an
// APLIC is to convert wired interrupts into MSIs for harts.
#[repr(C, packed)]
#[derive(Copy, Clone, Debug, AsBytes)]
pub struct APLIC {
    r#type: u8,
    length: u8,
    version: u8,
    _reserved: u8,
    aplic_id: U32,
    hardware_id: [u8; 8],
    number_of_idcs: U32,
    global_system_interrupt_base: U32,
    aplic_address: U64,
    aplic_size: U32,
    total_external_interrupt_sources: U16,
}

impl APLIC {
    pub fn new(
        aplic_id: u32,
        hardware_id: [u8; 8],
        number_of_idcs: u32,
        global_system_interrupt_base: u32,
        aplic_address: u64,
        aplic_size: u32,
        total_external_interrupt_sources: u16,
    ) -> Self {
        Self {
            r#type: RISCV_APLIC_STRUCTURE,
            length: Self::len() as u8,
            version: 1,
            _reserved: 0,
            aplic_id: aplic_id.into(),
            hardware_id,
            number_of_idcs: number_of_idcs.into(),
            global_system_interrupt_base: global_system_interrupt_base.into(),
            aplic_address: aplic_address.into(),
            aplic_size: aplic_size.into(),
            total_external_interrupt_sources: total_external_interrupt_sources.into(),
        }
    }

    pub fn len() -> usize {
        core::mem::size_of::<Self>()
    }
}

assert_same_size!(APLIC, [u8; 38]);
aml_as_bytes!(APLIC);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Aml;

    fn check_checksum(madt: &MADT) {
        let mut bytes = Vec::new();
        madt.to_aml_bytes(&mut bytes);
        let sum = bytes.iter().fold(0u8, |acc, x| acc.wrapping_add(*x));
        assert_eq!(sum, 0);
    }

    fn get_size(madt: &MADT) -> usize {
        let mut bytes = Vec::new();
        madt.to_aml_bytes(&mut bytes);
        bytes.len()
    }

    #[test]
    fn test_madt() {
        let madt = MADT::new(
            *b"FOOBAR",
            *b"DECAFCOF",
            0xdead_beef,
            LocalInterruptController::Riscv,
        );
        check_checksum(&madt);
        assert_eq!(Header::len(), get_size(&madt));
    }

    #[test]
    fn test_rintc() {
        let mut madt = MADT::new(
            *b"FOOBAR",
            *b"DECAFCOF",
            0xdead_beef,
            LocalInterruptController::Riscv,
        );
        check_checksum(&madt);
        assert_eq!(Header::len(), get_size(&madt));

        for i in 0..128 {
            let rintc = RINTC::new(HartStatus::Enabled, 42 + i as u64, (i + 0x1000) as u32);
            madt.add_rintc(rintc);
            check_checksum(&madt);
            assert_eq!(Header::len() + RINTC::len() * (i + 1), get_size(&madt));
        }
    }

    #[test]
    fn test_imsic() {
        let mut madt = MADT::new(
            *b"FOOBAR",
            *b"DECAFCOF",
            0xdead_beef,
            LocalInterruptController::Riscv,
        );
        check_checksum(&madt);
        assert_eq!(Header::len(), get_size(&madt));

        let imsic = IMSIC::new(
            10, /* num_supervisor_interrupt_identities */
            10, /* num_guest_interrupt_identities */
            8,  /* guest_index_bits */
            8,  /* hart_index_bits */
            8,  /* group_index_bits */
            8,  /* group_index_shift */
        );
        madt.add_imsic(imsic);
        check_checksum(&madt);
        assert_eq!(Header::len() + IMSIC::len(), get_size(&madt));
    }

    #[test]
    fn test_aplic() {
        let mut madt = MADT::new(
            *b"FOOBAR",
            *b"DECAFCOF",
            0xdead_beef,
            LocalInterruptController::Riscv,
        );
        check_checksum(&madt);
        assert_eq!(Header::len(), get_size(&madt));

        for i in 0..2 {
            let aplic = APLIC::new(
                0,                                       /* aplic_id */
                [b'A', b'B', b'C', b'D', b'E', 0, 0, 0], /* hardware_id */
                2,                                       /* number_of_idcs */
                0x8000_0000,                             /* global_system_interrupt_base */
                0x1_0000_0000,                           /* aplic_address */
                0x8192,                                  /* aplic_size */
                767,                                     /* total_external_interrupt_sources */
            );

            madt.add_aplic(aplic);
            check_checksum(&madt);
            assert_eq!(Header::len() + APLIC::len() * (i + 1), get_size(&madt));
        }
    }
}
