#![no_std]
#![no_main]
#![feature(proc_macro_hygiene)]
#![feature(ptr_to_from_bits)]
#![allow(unreachable_code)]

use core::ops::Range;
use core::panic::PanicInfo;

use aarch64_cpu::asm::wfe;
use log::LevelFilter;
use spin::Barrier;

use loader_payload_types::{Payload, PayloadInfo};
use sel4_platform_info::PLATFORM_INFO;

mod bcm2835_aux_uart;
mod copy_payload_data;
mod debug;
mod enter_kernel;
mod exception_handler;
mod fmt;
mod head;
mod init_platform_state;
mod init_translation_structures;
mod logging;
mod pl011;
mod plat;
mod psci;
mod smp;

use fmt::{debug_print, debug_println};
use logging::Logger;

const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

static LOGGER: Logger = Logger::new(LOG_LEVEL);

const MAX_NUM_NODES: usize = sel4_config::sel4_cfg_usize!(MAX_NUM_NODES);
const NUM_SECONDARY_CORES: usize = MAX_NUM_NODES - 1;

static KERNEL_ENTRY_BARRIER: Barrier = Barrier::new(MAX_NUM_NODES);

pub fn main<'a>(payload: &Payload<'a>, own_footprint: &Range<usize>) -> ! {
    debug::init();

    LOGGER.set().unwrap();

    log::debug!("Platform info: {:#x?}", PLATFORM_INFO);
    log::debug!("Loader footprint: {:#x?}", own_footprint);
    log::debug!("Payload info: {:#x?}", payload.info);
    log::debug!("Payload regions:");
    for content in payload.data.iter() {
        log::debug!(
            "    0x{:x?} {:?}",
            content.phys_addr_range,
            content.content.is_some()
        );
    }

    {
        let own_footprint =
            own_footprint.start.try_into().unwrap()..own_footprint.end.try_into().unwrap();
        loader_sanity_check::sanity_check(&own_footprint, &payload.data);
    }

    log::debug!("Copying payload data...");
    copy_payload_data::copy_payload_data(&payload.data);
    log::debug!("...done");

    {
        let kernel_phys_start = payload.info.kernel_image.phys_addr_range.start;
        let kernel_virt_start = payload.info.kernel_image.virt_addr_range().start;
        init_translation_structures::init_translation_structures(
            kernel_phys_start.try_into().unwrap(),
            kernel_virt_start.try_into().unwrap(),
        );
    }

    smp::start_secondary_cores(&payload.info);

    // NOTE
    // In elfloader, some of this init happens before secondary core bringup
    init_platform_state::init_platform_state_primary_core();

    common_epilogue(0, &payload.info)
}

fn secondary_core_main(core_id: usize, payload_info: &PayloadInfo) -> ! {
    common_epilogue(core_id, payload_info)
}

fn common_epilogue(core_id: usize, payload_info: &PayloadInfo) -> ! {
    init_platform_state::init_platform_state_per_core(core_id);
    log::info!("Core {}: entering kernel", core_id);
    KERNEL_ENTRY_BARRIER.wait();
    enter_kernel::enter_kernel(&payload_info);
    log::error!("Core {}: failed to enter kernel", core_id);
    idle()
}

//

#[panic_handler]
extern "C" fn panic_handler(info: &PanicInfo) -> ! {
    debug_println!("{}", info);
    idle()
}

fn idle() -> ! {
    loop {
        wfe();
    }
}
