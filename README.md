# AI Accelerator

A platform for emulating a PCI device in QEMU (with factorial support) and driving it with a Rust Linux kernel driver in a Docker container environment.

> Warning: According to [Rust for Linux](https://rust-for-linux.com/out-of-tree-modules#out-of-tree-modules), Rust internal APIs can be changed at any time. This means the current version of the driver may not work with the latest Linux kernel. Please check the compatibility when encountering build errors.

https://github.com/user-attachments/assets/a44ffe98-2d99-4de2-ba52-9628817a54cb

## Project Overview

- Docker Container - Ubuntu 24.04
- Target Architecture: aarch64 (ARM64)
    - aarch64-softmmu: Software MMU (Memory Management Unit) => Full System Emulation
- QEMU: Emulator but also VM; the Linux kernel with Rust support runs on it.
- `rootfs.ext4` (aarch64): Root Filesystem. It contains all the user-space files and directory structures required for Linux system startup and operation, as well as basic system tools.
- `drivers/`: The folder contains the Rust Linux kernel driver for the emulated PCI device. 

> The driver must use the exact same `rustc` version and `bindgen` configuration as the compiled Kernel.

## Quick Start

### 1. Setup Workspace Environment 

1. Run `docker build -t qemu-builder .` to build the image.

> Check the result by running `docker images`.

2. Start the Container

```bash
docker run -it -d --name emu -v $(pwd):/workspace -v linux-src-vol:/linux-env qemu-builder

# Go into the container
docker exec -it emu /bin/bash
```

### 2. Build QEMU Emulator with aarch64 Virtual Hardware Support

```bash
# In the container

cd qemu/

# Configure what target to compile
./configure --target-list=aarch64-softmmu --enable-debug

# Compile with all available cores
make -j$(nproc)

# Validate build
./build/qemu-system-aarch64 --version
```

### 3. Compile the PCI Rust Driver

```bash
# In the container
cd drivers/

# Get ai_accelerator.ko
make
```

### 4. Start the Machine

```bash
# In the container

cd /workspace

./qemu/build/qemu-system-aarch64 \
    -M virt \
    -cpu max \
    -m 1G \
    -nographic \
    -kernel ./Image \
    -drive format=raw,file=./rootfs.ext4 \
    -append "root=/dev/vda console=ttyAMA0" \
    -device ai-accelerator \
    -fsdev local,id=shareddev,path=/workspace,security_model=none \
    -device virtio-9p-device,fsdev=shareddev,mount_tag=shared
```

> Login: `root`, password: <Enter>

### 5. Mount & Load

After boot completes and you log in to the virtual system, perform the mount:

```bash
# 1. Create mount directory
mkdir -p /mnt

# 2. Perform mount (this time with the new kernel, the mount will succeed perfectly!)
mount -t 9p -o trans=virtio shared /mnt

# 3. Enter the driver folder in the shared directory
cd /mnt/drivers

# 4. Load the driver module
insmod ai_accelerator.ko

# 5. Run the factorial test via the device file
echo 5 > /dev/ai_accel
cat /dev/ai_accel # should output 120 (5!)
```

## Build Linux Kernel with Rust Support

> The kernel version I tested was at commit 11439c4635 (2026/03/01)

> Remark: Follow https://docs.kernel.org/rust/quick-start.html to set up

1. Start the Container

```bash
docker volume ls

# Since we cannot compile the Linux kernel in the workspace, we need to create a Docker volume to store the compiled kernel and share it with the container.
docker volume create linux-src-vol

docker run -it -d --name emu -v $(pwd):/workspace -v linux-src-vol:/linux-env qemu-builder

# Go into the container
docker exec -it emu /bin/bash
```

2. Go to the folder (here: linux-env/) for the Linux kernel source code

```bash
# In the container

cd linux-env/
git clone --depth=10 https://github.com/Rust-for-Linux/linux.git
cd linux/
```

3. Install Rust toolchain and dependencies

```bash
# In the container

apt-get update && apt install -y rustc-1.85 rust-1.85-src bindgen-0.71 rustfmt-1.85 rust-1.85-clippy
ln -s /usr/lib/rust-1.85/bin/rustfmt /usr/bin/rustfmt-1.85
ln -s /usr/lib/rust-1.85/bin/clippy-driver /usr/bin/clippy-driver-1.85
PATH=/usr/lib/rust-1.85/bin:$PATH
update-alternatives --install /usr/bin/bindgen bindgen /usr/bin/bindgen-0.71 100
update-alternatives --set bindgen /usr/bin/bindgen-0.71
RUST_LIB_SRC=/usr/src/rustc-$(rustc-1.85 --version | cut -d' ' -f2)/library
apt -y install clang lld llvm
export LIBCLANG_PATH=/usr/lib/llvm-18/lib

# The "Rust is available!" output must be confirmed before compiling the kernel
make LLVM=1 rustavailable
```

4. Turn on Rust support in the kernel configuration

Run `make ARCH=arm64 LLVM=1 menuconfig` and enable `Rust support` 

5. Add QEMU virtual vendor ID in the kernel source code

```rust
// In linux/rust/kernel/pci/id.rs, add

/// QEMU virtual vendor ID used by emulated devices.
pub const QEMU: Self = Self(0x1234);
```

6. Compile the kernel

```bash
make LLVM=1
```

6. Copy the compiled kernel image to the workspace

## (Optional) Add a new PCI Device in Qemu

1. In `qemu/hw/misc`, copy `edu.c` and rename it.
2. Add `system_ss.add(files('<new_file_name>'))` in `hw/misc/meson.build`.

## Troubleshooting

### Qemu

- To terminate QEMU, use `Ctrl + a` then `x` in the terminal where QEMU is running.

## TODO

- [x] Factorial Polling Test
    - [x] Write factorial input value to `Regs::FACT`.
    - [x] Poll `Regs::STATUS` until `STATUS_COMPUTING` (0x01) is cleared.
    - [x] Read back the computed result from `Regs::FACT` and validate.
- [x] Character Device & File Operations
    - [x] Register character device `/dev/ai_accel`.
    - [x] Implement `file::Operations` kernel trait (`read`/`write` methods) to interface with user space.
- [ ] Factorial Interrupt (IRQ) Test
    - [ ] Enable MSI (Message Signaled Interrupts) in the Rust driver.
    - [ ] Register an interrupt handler (ISR) in the driver.
    - [ ] Trigger and handle interrupts on computation completion or via test registers.
- [ ] Coherent DMA Integration
    - [ ] Allocate Coherent DMA buffer in the Rust driver.
    - [ ] Map DMA addresses and configure `DMA_SRC`, `DMA_DST`, and `DMA_CNT` registers.
    - [ ] Trigger DMA transfers and synchronize with the interrupt system.
- [ ] Matrix Multiplication implementation
    - [ ] Modify QEMU C code to replace the factorial thread with matrix multiplication logic.
    - [ ] Update the Rust driver to handle matrix dimensions and computation status.
- (Optional) Implement a runtime library that allows user programs to interact with the driver and program it.

## References

- [Setup QEMU build environment](https://www.qemu.org/docs/master/devel/build-environment.html)
- [Linux Kernel for Rust official quick start](https://docs.kernel.org/rust/quick-start.html)
- [rust-out-of-tree-module](https://github.com/Rust-for-Linux/rust-out-of-tree-module)
- [Mentorship Session: Setting Up an Environment for Writing Linux Kernel Modules in Rust](https://youtu.be/tPs1uRqOnlk?si=vv0MUz0EiAsrHTGv)
- [Build Linux Rust PCI Driver](https://hackmd.io/@tQN1jUM6TwaU156s8AO7kQ/SyZvf-k2Zx)
- [(See also) Rust general information in Rust](https://docs.kernel.org/rust/general-information.html)
- [(See also) Rust Distribution Repository](https://static.rust-lang.org/dist/2024-05-02/index.html)
- [(See also) Nvidia Virtual Platform](https://nvdla.org/vp.html)
- [(See also) Building a Linux Kernel Driver using Rust](https://rust-exercises.ferrous-systems.com/latest/book/building-linux-kernel-driver)
- [(See also) Write your first linux kernel module in c](https://medium.com/dvt-engineering/how-to-write-your-first-linux-kernel-module-cf284408beeb)

## Other Commands

- `make clean`
- `make ARCH=arm64 distclean`
- Check docker volume size: `docker system df -v`
