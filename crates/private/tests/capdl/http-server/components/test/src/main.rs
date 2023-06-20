#![no_std]
#![no_main]
#![feature(slice_ptr_get)]
#![feature(strict_provenance)]
#![feature(never_type)]
#![feature(pattern)]

extern crate alloc;

use core::ops::Range;
use core::ptr::NonNull;

use serde::{Deserialize, Serialize};

use virtio_drivers::{
    device::net::*,
    transport::{
        mmio::{MmioTransport, VirtIOHeader},
        DeviceType, Transport,
    },
};

use sel4_logging::{LevelFilter, Logger, LoggerBuilder};
use sel4_simple_task_config_types::*;
use sel4_simple_task_runtime::main_json;
use tests_capdl_http_server_components_test_sp804_driver::Driver;

mod ctx;
mod net;
mod test;

use net::{HalImpl, Net};

// const LOG_LEVEL: LevelFilter = LevelFilter::Trace;
// const LOG_LEVEL: LevelFilter = LevelFilter::Debug;
const LOG_LEVEL: LevelFilter = LevelFilter::Info;
// const LOG_LEVEL: LevelFilter = LevelFilter::Warn;

static LOGGER: Logger = LoggerBuilder::const_default()
    .level_filter(LOG_LEVEL)
    .filter(|meta| !meta.target().starts_with("sel4_sys"))
    .write(|s| sel4::debug_print!("{}", s))
    .build();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub event_nfn: ConfigCPtr<Notification>,
    pub timer_irq_handler: ConfigCPtr<IRQHandler>,
    pub timer_mmio_vaddr: usize,
    pub timer_freq: usize,
    pub virtio_net_irq_handler: ConfigCPtr<IRQHandler>,
    pub virtio_net_mmio_vaddr: usize,
    pub virtio_net_mmio_offset: usize,
    pub virtio_net_dma_vaddr_range: Range<usize>,
    pub virtio_net_dma_vaddr_to_paddr_offset: isize,
}

const NET_BUFFER_LEN: usize = 2048;

#[main_json]
fn main(config: Config) {
    LOGGER.set().unwrap();

    // debug_println!("{:#x?}", config);

    let timer = unsafe {
        Driver::new(
            config.timer_mmio_vaddr as *mut (),
            config.timer_freq.try_into().unwrap(),
        )
    };

    HalImpl::init(
        NonNull::slice_from_raw_parts(
            NonNull::new(config.virtio_net_dma_vaddr_range.start as *mut _).unwrap(),
            config.virtio_net_dma_vaddr_range.end - config.virtio_net_dma_vaddr_range.start,
        ),
        config.virtio_net_dma_vaddr_to_paddr_offset,
    );

    let net = {
        let header = NonNull::new(
            (config.virtio_net_mmio_vaddr + config.virtio_net_mmio_offset) as *mut VirtIOHeader,
        )
        .unwrap();
        let transport = unsafe { MmioTransport::new(header) }.unwrap();
        assert_eq!(transport.device_type(), DeviceType::Network);
        Net::new(VirtIONet::new(transport, NET_BUFFER_LEN).unwrap())
    };

    let this_ctx = ctx::Ctx::new(
        timer,
        config.timer_irq_handler.get(),
        net,
        config.virtio_net_irq_handler.get(),
    );

    this_ctx.run(config.event_nfn.get(), test::test);

    // debug_println!("TEST_PASS");
}
