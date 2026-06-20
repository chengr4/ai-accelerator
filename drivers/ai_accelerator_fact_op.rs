//! AI Accelerator Rust Driver.

use kernel::{device::Core, devres::Devres, io::Io, pci, prelude::*, sync::aref::ARef};

/// Hardware register offsets for the AI Accelerator.
struct Regs;

impl Regs {
    const MAGIC: usize = 0x00;
    const ADDR4: usize = 0x04;
    const FACT: usize = 0x08;
    const STATUS: usize = 0x20;
    const IRQ_STATUS: usize = 0x24;
    const SCRATCH: usize = 0x28;

    // Interrupt Test Registers
    const IRQ_RAISE: usize = 0x60;
    const IRQ_LOWER: usize = 0x64;

    // DMA
    const DMA_SRC: usize = 0x80;
    const DMA_DST: usize = 0x88;
    const DMA_CNT: usize = 0x90;
    const DMA_CMD: usize = 0x98;

    // Boundary
    const END: usize = 0x100000;
}

// Define from device side
const STATUS_COMPUTING: u32 = 0x01;

/// Type alias for the Control MMIO region with bounds checking.
type Bar0 = pci::Bar<{ Regs::END }>;

#[pin_data(PinnedDrop)]
struct AIAcceleratorDriver {
    pdev: ARef<pci::Device>,
    #[pin]
    bar: Devres<Bar0>,
}

kernel::pci_device_table!(
    PCI_TABLE,
    MODULE_PCI_TABLE,
    <AIAcceleratorDriver as pci::Driver>::IdInfo,
    [(pci::DeviceId::from_id(pci::Vendor::QEMU, 0x11e9), ())]
);

impl pci::Driver for AIAcceleratorDriver {
    type IdInfo = ();
    const ID_TABLE: pci::IdTable<Self::IdInfo> = &PCI_TABLE;

    fn probe(pdev: &pci::Device<Core>, _info: &Self::IdInfo) -> impl PinInit<Self, Error> {
        pin_init::pin_init_scope(move || {
            pdev.enable_device_mem()?;
            pdev.set_master();

            Ok(try_pin_init!(Self {
                // Map BAR 0 (index 0) into kernel virtual memory.
                bar <- pdev.iomap_region_sized::<{ Regs::END }>(0, c"ai_accelerator"),

                // Perform hardware validation before completing initialization.
                _: {
                    let regs_access = bar.access(pdev.as_ref())?;
                    let magic = regs_access.read32(Regs::MAGIC);

                    if magic != 0x010000a1 {
                        dev_err!(pdev, "AI Accelerator hardware identification failed. Magic: 0x{:08x}\n", magic);
                        return Err(ENODEV);
                    }

                    dev_info!(pdev, "AI Accelerator hardware initialized. Magic: 0x{:08x}\n", magic);

                    // NOTE: The following polling test is for quick hardware validation only.
                    // In a production driver, probe must follow the Single Responsibility Principle
                    // and only handle device initialization. Daily operations should be triggered
                    // via callbacks initiated from the user space (e.g., character device file operations).
                    let test_val: u32 = 5;
                    dev_info!(pdev, "Starting factorial polling test for value: {}\n", test_val);

                    regs_access.write32(test_val, Regs::FACT);

                    // Poll the STATUS register until the COMPUTING flag is cleared (becomes 0)
                    let mut timeout_counter = 1_000_000;
                    let mut status = regs_access.read32(Regs::STATUS);

                    while (status & STATUS_COMPUTING) != 0 {
                        if timeout_counter == 0 {
                            dev_err!(pdev, "Factorial computation timed out.\n");
                            return Err(EIO);
                        }
                        timeout_counter -= 1;
                        core::hint::spin_loop(); // Reduce CPU resource consumption during busy waiting
                        status = regs_access.read32(Regs::STATUS);
                    }

                    let result = regs_access.read32(Regs::FACT);
                    dev_info!(pdev, "Factorial computation completed. Result: {}! = {}\n", test_val, result);
                    if result != 120 {
                        dev_err!(pdev, "Factorial computation error. Expected 120, got {}.\n", result);
                        return Err(EIO);
                    }

                    dev_info!(pdev, "Factorial test passed.\n");
                },

                pdev: pdev.into(),
            }))
        })
    }

    // TODO: Implement unbind to clean up resources when the device is removed.
    // fn unbind(pdev: &pci::Device<Core>, this: Pin<&Self>) {
    //     if let Ok(bar) = this.bar.access(pdev.as_ref()) {
    //         // Reset pci-testdev by writing a new test index.
    //         bar.write8(this.index.0, Regs::TEST);
    //     }
    // }
}

#[pinned_drop]
impl PinnedDrop for AIAcceleratorDriver {
    fn drop(self: Pin<&mut Self>) {
        dev_dbg!(self.pdev, "Dropping AI Accelerator driver resources.\n");
    }
}

kernel::module_pci_driver! {
    type: AIAcceleratorDriver,
    name: "ai_accelerator",
    authors: ["R4 Cheng"],
    description: "Rust AI Accelerator PCI Driver",
    license: "GPL v2",
}
