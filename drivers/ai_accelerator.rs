//! AI Accelerator Rust Driver.

use kernel::{
    device::Core,
    devres::Devres,
    fs::{File, Kiocb},
    io::Io,
    miscdevice::{self, MiscDeviceRegistration, MiscDeviceOptions},
    pci,
    prelude::*,
    sync::aref::ARef,
    alloc::allocator::Kmalloc,
    iov::{IovIterDest, IovIterSource},
};

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

/// Status register mask: indicates the hardware is currently performing factorial computation.
const STATUS_COMPUTING: u32 = 0x01;

/// Type alias for the Control MMIO region with bounds checking.
type Bar0 = pci::Bar<{ Regs::END }>;

// =====================================================================
// Global State for Character Device Access
// =====================================================================

/// Global raw pointer to access the pinned Bar0 memory within file operations.
static mut GLOBAL_IO: Option<*const Bar0> = None;

/// Global variable to temporarily store the last computation result.
static mut COMPUTATION_RESULT: u32 = 0;

// =====================================================================
// Helper Function: Convert u32 to dec string without alloc (no_std)
// =====================================================================
fn u32_to_dec_str(mut num: u32, buf: &mut [u8]) -> usize {
    if num == 0 {
        buf[0] = b'0';
        buf[1] = b'\n';
        return 2;
    }
    let mut temp = [0u8; 10];
    let mut i = 0;
    while num > 0 {
        temp[i] = b'0' + (num % 10) as u8;
        num /= 10;
        i += 1;
    }
    let mut j = 0;
    while j < i {
        buf[j] = temp[i - 1 - j];
        j += 1;
    }
    buf[j] = b'\n';
    j + 1
}

// =====================================================================
// Character Device File Operations Implementation
// =====================================================================

struct AIAcceleratorDevice;

#[vtable]
impl miscdevice::MiscDevice for AIAcceleratorDevice {
    type Ptr = Box<Self, Kmalloc>;

    /// Called when the misc device is opened.
    fn open(_file: &File, _misc: &MiscDeviceRegistration<Self>) -> Result<Self::Ptr> {
        Ok(Box::new(AIAcceleratorDevice, GFP_KERNEL)?)
    }

    /// Read from this miscdevice (e.g. cat /dev/ai_accel).
    fn read_iter(kiocb: Kiocb<'_, Self::Ptr>, iov: &mut IovIterDest<'_>) -> Result<usize> {
        // SAFETY: Safe to read the file position offset from the raw kiocb.
        let pos = unsafe { (*kiocb.as_raw()).ki_pos };

        // If we have already read the data, return EOF (0 bytes) to terminate the read loop.
        if pos > 0 {
            return Ok(0);
        }

        let result = unsafe { COMPUTATION_RESULT };
        let mut buf = [0u8; 32];
        let len = u32_to_dec_str(result, &mut buf);
        
        iov.copy_to_iter(&buf[..len]);

        // Advance the file position so the next read call receives EOF.
        unsafe {
            (*kiocb.as_raw()).ki_pos += len as i64;
        }

        Ok(len)
    }

    /// Write to this miscdevice (e.g. echo 5 > /dev/ai_accel).
    fn write_iter(_kiocb: Kiocb<'_, Self::Ptr>, iov: &mut IovIterSource<'_>) -> Result<usize> {
        let mut buf = [0u8; 32];
        let len = iov.copy_from_iter(&mut buf);

        // Parse user input string to u32.
        let input_str = core::str::from_utf8(&buf[..len])
            .map_err(|_| EINVAL)?
            .trim();

        let val = input_str.parse::<u32>().map_err(|_| EINVAL)?;

        // Acquire the MMIO register and start computation.
        unsafe {
            if let Some(bar_ptr) = GLOBAL_IO {
                let io: &Bar0 = &*bar_ptr;
                
                // 1. Write the value to calculate to the FACT register.
                io.write32(val, Regs::FACT);

                // 2. Poll the STATUS register until the COMPUTING flag is cleared.
                let mut timeout_counter = 1_000_000;
                let mut status = io.read32(Regs::STATUS);

                while (status & STATUS_COMPUTING) != 0 {
                    if timeout_counter == 0 {
                        return Err(EIO);
                    }
                    timeout_counter -= 1;
                    core::hint::spin_loop(); // Reduce CPU resource consumption during busy waiting.
                    status = io.read32(Regs::STATUS);
                }

                // 3. Read back the computed result and store it globally.
                let result = io.read32(Regs::FACT);
                COMPUTATION_RESULT = result;
            } else {
                return Err(ENODEV);
            }
        }

        Ok(len)
    }
}

// =====================================================================
// PCI Driver Life Cycle Implementation
// =====================================================================

#[pin_data(PinnedDrop)]
struct AIAcceleratorDriver {
    pdev: ARef<pci::Device>,
    #[pin]
    bar: Devres<Bar0>,
    // Pinned misc device registration handle for RAII resource cleanup.
    misc_dev: Pin<Box<MiscDeviceRegistration<AIAcceleratorDevice>, Kmalloc>>,
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
                // 1. Map BAR 0 (index 0) into kernel virtual memory.
                bar <- pdev.iomap_region_sized::<{ Regs::END }>(0, c"ai_accelerator"),

                // 2. Perform hardware identification and dynamically register character device.
                misc_dev: {
                    let regs_access = bar.access(pdev.as_ref())?;
                    let magic = regs_access.read32(Regs::MAGIC);

                    if magic != 0x010000a1 {
                        dev_err!(pdev, "AI Accelerator hardware identification failed. Magic: 0x{:08x}\n", magic);
                        return Err(ENODEV);
                    }
                    dev_info!(pdev, "AI Accelerator hardware initialized. Magic: 0x{:08x}\n", magic);

                    // Cache the MMIO access interface globally for file operations.
                    unsafe {
                        GLOBAL_IO = Some(regs_access as *const Bar0);
                    }

                    // Dynamically register the miscellaneous character device node (/dev/ai_accel).
                    Box::pin_init(
                        MiscDeviceRegistration::<AIAcceleratorDevice>::register(
                            MiscDeviceOptions { name: c"ai_accel" }
                        ),
                        GFP_KERNEL
                    )?
                },

                pdev: pdev.into(),
            }))
        })
    }
}

#[pinned_drop]
impl PinnedDrop for AIAcceleratorDriver {
    fn drop(self: Pin<&mut Self>) {
        dev_dbg!(self.pdev, "Dropping AI Accelerator driver resources.\n");
        // Clear the global MMIO reference upon driver removal to prevent use-after-free.
        unsafe {
            GLOBAL_IO = None;
        }
    }
}

kernel::module_pci_driver! {
    type: AIAcceleratorDriver,
    name: "ai_accelerator",
    authors: ["R4 Cheng"],
    description: "Rust AI Accelerator PCI Driver with Misc Char Device",
    license: "GPL v2",
}
