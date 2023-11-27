// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) Microsoft Corporation
//
// Author: Jon Lange (jlange@microsoft.com)

extern crate alloc;

use crate::address::{PhysAddr, VirtAddr};
use crate::error::SvsmError;
use crate::error::SvsmError::Firmware;
use crate::mm::PAGE_SIZE;
use crate::utils::MemoryRegion;
use alloc::vec::Vec;

use core::mem::size_of;
use igvm_defs::{MemoryMapEntryType, IGVM_VHS_MEMORY_MAP_ENTRY};
use igvm_params::{IgvmParamBlock, IgvmParamPage};

const IGVM_MEMORY_ENTRIES_PER_PAGE: usize = PAGE_SIZE / size_of::<IGVM_VHS_MEMORY_MAP_ENTRY>();

#[derive(Clone, Debug)]
#[repr(C, align(64))]
pub struct IgvmMemoryMap {
    memory_map: [IGVM_VHS_MEMORY_MAP_ENTRY; IGVM_MEMORY_ENTRIES_PER_PAGE],
}

#[derive(Clone, Debug)]
pub struct IgvmParams<'a> {
    igvm_param_block: &'a IgvmParamBlock,
    igvm_param_page: &'a IgvmParamPage,
    igvm_memory_map: &'a IgvmMemoryMap,
}

impl IgvmParams<'_> {
    pub fn new(addr: VirtAddr) -> Self {
        let param_block = unsafe { &*addr.as_ptr::<IgvmParamBlock>() };
        let param_page_address = addr + param_block.param_page_offset.try_into().unwrap();
        let param_page = unsafe { &*param_page_address.as_ptr::<IgvmParamPage>() };
        let memory_map_address = addr + param_block.memory_map_offset.try_into().unwrap();
        let memory_map = unsafe { &*memory_map_address.as_ptr::<IgvmMemoryMap>() };

        Self {
            igvm_param_block: param_block,
            igvm_param_page: param_page,
            igvm_memory_map: memory_map,
        }
    }

    pub fn size(&self) -> usize {
        // Calculate the total size of the parameter area.  The
        // parameter area always begins at the kernel base
        // address.
        self.igvm_param_block.param_area_size.try_into().unwrap()
    }

    pub fn find_kernel_region(&self) -> Result<MemoryRegion<PhysAddr>, SvsmError> {
        let kernel_base = PhysAddr::from(self.igvm_param_block.kernel_base);
        let kernel_size: usize = self.igvm_param_block.kernel_size.try_into().unwrap();
        Ok(MemoryRegion::<PhysAddr>::new(kernel_base, kernel_size))
    }

    pub fn page_state_change_required(&self) -> bool {
        self.igvm_param_page.default_shared_pages != 0
    }

    pub fn get_cpuid_page_address(&self) -> u64 {
        self.igvm_param_block.cpuid_page as u64
    }

    pub fn get_secrets_page_address(&self) -> u64 {
        self.igvm_param_block.secrets_page as u64
    }

    pub fn get_memory_regions(&self) -> Result<Vec<MemoryRegion<PhysAddr>>, SvsmError> {
        // Count the number of memory entries present.  They must be
        // non-overlapping and strictly increasing.
        let mut number_of_entries = 0;
        let mut next_page_number = 0;
        for i in 0..IGVM_MEMORY_ENTRIES_PER_PAGE {
            let entry = &self.igvm_memory_map.memory_map[i];
            if entry.number_of_pages == 0 {
                break;
            }
            if entry.starting_gpa_page_number < next_page_number {
                return Err(Firmware);
            }
            let next_supplied_page_number = entry.starting_gpa_page_number + entry.number_of_pages;
            if next_supplied_page_number < next_page_number {
                return Err(Firmware);
            }
            next_page_number = next_supplied_page_number;
            number_of_entries += 1;
        }

        // Now loop over the supplied entires and add a region for each
        // known type.
        let mut regions: Vec<MemoryRegion<PhysAddr>> = Vec::new();
        for i in 0..number_of_entries {
            let entry = &self.igvm_memory_map.memory_map[i];
            if entry.entry_type == MemoryMapEntryType::MEMORY {
                let starting_page: usize = entry.starting_gpa_page_number.try_into().unwrap();
                let number_of_pages: usize = entry.number_of_pages.try_into().unwrap();
                regions.push(MemoryRegion::new(
                    PhysAddr::new(starting_page * PAGE_SIZE),
                    number_of_pages * PAGE_SIZE,
                ));
            }
        }

        Ok(regions)
    }
}
