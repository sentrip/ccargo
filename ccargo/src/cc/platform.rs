use crate::utils::{IResult, lev_distance};
use std::collections::HashSet;

pub use ccargo_platform::RustcTarget;

// PRIMARY TARGETS
//  aarch64-unknown-linux-gnu      ARM64 Linux (kernel 4.1, glibc 2.17+)
//  i686-pc-windows-msvc           32-bit MSVC (Windows 7+)
//  i686-pc-windows-gnu            32-bit MinGW (Windows 7+)
//  i686-pc-windows-gnullvm        32-bit Clang (Windows 7+)
//  i686-unknown-linux-gnu         32-bit Linux (kernel 3.2+, glibc 2.17+)
//  x86_64-apple-darwin            64-bit macOS (10.7+, Lion+)
//  x86_64-pc-windows-msvc         64-bit MSVC (Windows 7+)
//  x86_64-pc-windows-gnu          64-bit MinGW (Windows 7+)
//  x86_64-pc-windows-gnullvm      64-bit Clang (Windows 7+)
//  x86_64-unknown-linux-gnu       64-bit Linux (kernel 3.2+, glibc 2.17+)

// Get the host target platform
pub fn host_platform() -> &'static RustcTarget {
    lazy_static::lazy_static! { 
        static ref HOST: RustcTarget = RustcTarget::detect()
            .unwrap_or(RustcTarget::new(host_triple().to_string(), Vec::new()));
    }    
    &HOST
}


// Get the host target triple
pub fn host_triple() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_arch = "x86_64") {
            "x86_64-pc-windows-msvc"
        } else {
            "i686-pc-windows-msvc"
        }
    } else if cfg!(apple) {
        "x86_64-apple-darwin"
    } else {
        if cfg!(target_arch = "aarch64") {
            "aarch64-unknown-linux-gnu"
        } else if cfg!(target_arch = "x86_64") {
            "x86_64-unknown-linux-gnu"
        } else {
            "i686-unknown-linux-gnu"
        }
    }
}


// Validate target identifier
pub fn validate_target(target: &str) -> IResult<()> {
    if !VALID_TARGETS.contains(target) {
        anyhow::bail!(
            "Invalid target `{target}`{}",
            lev_distance::closest_msg(
                target, 
                VALID_TARGETS.iter(), 
                |v| &v[..]
            )
        )
    }
    return Ok(());
}

lazy_static::lazy_static! { 
    // we want to make a nice table
    #[rustfmt::skip]
    static ref VALID_TARGETS: HashSet<&'static str> = HashSet::from([
        // Tier 1 with host tools
        "aarch64-unknown-linux-gnu",            // ARM64  Linux (kernel 4.1, glibc 2.17+)
        "i686-pc-windows-msvc",                 // 32-bit MSVC  (Windows 7+)
        "i686-pc-windows-gnu",                  // 32-bit MinGW (Windows 7+)
        "i686-pc-windows-gnullvm",              // 32-bit Clang (Windows 7+)
        "i686-unknown-linux-gnu",               // 32-bit Linux (kernel 3.2+, glibc 2.17+)
        "x86_64-apple-darwin",                  // 64-bit macOS (10.7+, Lion+)
        "x86_64-pc-windows-msvc",               // 64-bit MSVC  (Windows 7+)
        "x86_64-pc-windows-gnu",                // 64-bit MinGW (Windows 7+)
        "x86_64-pc-windows-gnullvm",            // 64-bit Clang (Windows 7+)
        "x86_64-unknown-linux-gnu",             // 64-bit Linux (kernel 3.2+, glibc 2.17+)
        
        // Tier 2 with host tools
        "aarch64-apple-darwin",                 // ARM64 macOS (11.0+, Big Sur+)
        "aarch64-pc-windows-msvc",              // ARM64 Windows MSVC
        "aarch64-unknown-linux-musl",           // ARM64 Linux with MUSL
        "arm-unknown-linux-gnueabi",            // ARMv6 Linux (kernel 3.2, glibc 2.17)
        "arm-unknown-linux-gnueabihf",          // ARMv6 Linux, hardfloat (kernel 3.2, glibc 2.17)
        "armv7-unknown-linux-gnueabihf",        // ARMv7 Linux, hardfloat (kernel 3.2, glibc 2.17)
        "mips-unknown-linux-gnu",               // MIPS Linux (kernel 4.4, glibc 2.23)
        "mips64-unknown-linux-gnuabi64",        // MIPS64 Linux, n64 ABI (kernel 4.4, glibc 2.23)
        "mips64el-unknown-linux-gnuabi64",      // MIPS64 (LE) Linux, n64 ABI (kernel 4.4, glibc 2.23)
        "mipsel-unknown-linux-gnu",             // MIPS (LE) Linux (kernel 4.4, glibc 2.23)
        "powerpc-unknown-linux-gnu",            // PowerPC Linux (kernel 3.2, glibc 2.17)
        "powerpc64-unknown-linux-gnu",          // PPC64 Linux (kernel 3.2, glibc 2.17)
        "powerpc64le-unknown-linux-gnu",        // PPC64LE Linux (kernel 3.10, glibc 2.17)
        "riscv64gc-unknown-linux-gnu",          // RISC-V Linux (kernel 4.20, glibc 2.29)
        "s390x-unknown-linux-gnu",              // S390x Linux (kernel 3.2, glibc 2.17)
        "x86_64-unknown-freebsd",               // 64-bit FreeBSD
        "x86_64-unknown-illumos",               // illumos
        "x86_64-unknown-linux-musl",            // 64-bit Linux with MUSL
        "x86_64-unknown-netbsd",                // NetBSD/amd64

        // Tier 2
        "aarch64-apple-ios",                    // ARM64 iOS
        "aarch64-apple-ios-sim",                // Apple iOS Simulator on ARM64
        "aarch64-fuchsia",                      // Alias for aarch64-unknown-fuchsia
        "aarch64-unknown-fuchsia",              // ARM64 Fuchsia
        "aarch64-linux-android",                // ARM64 Android
        "aarch64-unknown-none-softfloat",       // Bare ARM64, softfloat
        "aarch64-unknown-none",                 // Bare ARM64, hardfloat
        "aarch64-unknown-uefi",                 // ARM64 UEFI
        "arm-linux-androideabi",                // ARMv7 Android
        "arm-unknown-linux-musleabi",           // ARMv6 Linux with MUSL
        "arm-unknown-linux-musleabihf",         // ARMv6 Linux with MUSL, hardfloat
        "armebv7r-none-eabi",                   // Bare ARMv7-R, Big Endian
        "armebv7r-none-eabihf",                 // Bare ARMv7-R, Big Endian, hardfloat
        "armv5te-unknown-linux-gnueabi",        // ARMv5TE Linux (kernel 4.4, glibc 2.23)
        "armv5te-unknown-linux-musleabi",       // ARMv5TE Linux with MUSL
        "armv7-linux-androideabi",              // ARMv7a Android
        "armv7-unknown-linux-gnueabi",          // ARMv7 Linux (kernel 4.15, glibc 2.27)
        "armv7-unknown-linux-musleabi",         // ARMv7 Linux with MUSL
        "armv7-unknown-linux-musleabihf",       // ARMv7 Linux with MUSL, hardfloat
        "armv7a-none-eabi",                     // Bare ARMv7-A
        "armv7r-none-eabi",                     // Bare ARMv7-R
        "armv7r-none-eabihf",                   // Bare ARMv7-R, hardfloat
        "asmjs-unknown-emscripten",             // asm.js via Emscripten
        "i586-pc-windows-msvc",                 // 32-bit Windows w/o SSE
        "i586-unknown-linux-gnu",               // 32-bit Linux w/o SSE (kernel 3.2, glibc 2.17)
        "i586-unknown-linux-musl",              // 32-bit Linux w/o SSE, MUSL
        "i686-linux-android",                   // 32-bit x86 Android
        "i686-unknown-freebsd",                 // 32-bit FreeBSD
        "i686-unknown-linux-musl",              // 32-bit Linux with MUSL
        "i686-unknown-uefi",                    // 32-bit UEFI
        "mips-unknown-linux-musl",              // MIPS Linux with MUSL
        "mips64-unknown-linux-muslabi64",       // MIPS64 Linux, n64 ABI, MUSL
        "mips64el-unknown-linux-muslabi64",     // MIPS64 (LE) Linux, n64 ABI, MUSL
        "mipsel-unknown-linux-musl",            // MIPS (LE) Linux with MUSL
        "nvptx64-nvidia-cuda",                  // --emit=asm generates PTX code that runs on NVIDIA GPUs
        "riscv32i-unknown-none-elf",            // Bare RISC-V (RV32I ISA)
        "riscv32imac-unknown-none-elf",         // Bare RISC-V (RV32IMAC ISA)
        "riscv32imc-unknown-none-elf",          // Bare RISC-V (RV32IMC ISA)
        "riscv64gc-unknown-none-elf",           // Bare RISC-V (RV64IMAFDC ISA)
        "riscv64imac-unknown-none-elf",         // Bare RISC-V (RV64IMAC ISA)
        "sparc64-unknown-linux-gnu",            // SPARC Linux (kernel 4.4, glibc 2.23)
        "sparcv9-sun-solaris",                  // SPARC Solaris 10/11, illumos
        "thumbv6m-none-eabi",                   // Bare Cortex-M0, M0+, M1
        "thumbv7em-none-eabi",                  // Bare Cortex-M4, M7
        "thumbv7em-none-eabihf",                // Bare Cortex-M4F, M7F, FPU, hardfloat
        "thumbv7m-none-eabi",                   // Bare Cortex-M3
        "thumbv7neon-linux-androideabi",        // Thumb2-mode ARMv7a Android with NEON
        "thumbv7neon-unknown-linux-gnueabihf",  // Thumb2-mode ARMv7a Linux with NEON (kernel 4.4, glibc 2.23)
        "thumbv8m.base-none-eabi",              // ARMv8-M Baseline
        "thumbv8m.main-none-eabi",              // ARMv8-M Mainline
        "thumbv8m.main-none-eabihf",            // ARMv8-M Mainline, hardfloat
        "wasm32-unknown-emscripten",            // WebAssembly via Emscripten
        "wasm32-unknown-unknown",               // WebAssembly
        "wasm32-wasi",                          // WebAssembly with WASI
        "x86_64-apple-ios",                     // 64-bit x86 iOS
        "x86_64-fortanix-unknown-sgx",          // Fortanix ABI for 64-bit Intel SGX
        "x86_64-fuchsia",                       // Alias for x86_64-unknown-fuchsia
        "x86_64-unknown-fuchsia",               // 64-bit Fuchsia
        "x86_64-linux-android",                 // 64-bit x86 Android
        "x86_64-pc-solaris",                    // 64-bit Solaris 10/11, illumos
        "x86_64-unknown-linux-gnux32",          // 64-bit Linux (x32 ABI) (kernel 4.15, glibc 2.27)
        "x86_64-unknown-none",                  // Freestanding/bare-metal x86_64, softfloat
        "x86_64-unknown-redox",                 // Redox OS
        "x86_64-unknown-uefi",                  // 64-bit UEFI

        // Tier 3
        "aarch64-apple-ios-macabi",             // Apple Catalyst on ARM64
        "aarch64-apple-tvos",                   // ARM64 tvOS
        "aarch64-apple-watchos-sim",            // ARM64 Apple WatchOS Simulator
        "aarch64-kmc-solid_asp3",               // ARM64 SOLID with TOPPERS/ASP3
        "aarch64-nintendo-switch-freestanding", // ARM64 Nintendo Switch, Horizon
        "aarch64-pc-windows-gnullvm",
        "aarch64-unknown-nto-qnx710",           // ARM64 QNX Neutrino 7.1 RTOS
        "aarch64-unknown-freebsd",              // ARM64 FreeBSD
        "aarch64-unknown-hermit",               // ARM64 HermitCore
        "aarch64-unknown-linux-gnu_ilp32",      // ARM64 Linux (ILP32 ABI)
        "aarch64-unknown-netbsd",
        "aarch64-unknown-openbsd",              // ARM64 OpenBSD
        "aarch64-unknown-redox",                // ARM64 Redox OS
        "aarch64-uwp-windows-msvc",
        "aarch64-wrs-vxworks",
        "aarch64_be-unknown-linux-gnu_ilp32",   // ARM64 Linux (big-endian, ILP32 ABI)
        "aarch64_be-unknown-linux-gnu",         // ARM64 Linux (big-endian)
        "arm64_32-apple-watchos",               // ARM Apple WatchOS 64-bit with 32-bit pointers
        "armeb-unknown-linux-gnueabi",          // ARM BE8 the default ARM big-endian architecture since ARMv6.
        "armv4t-none-eabi",                     // ARMv4T A32
        "armv4t-unknown-linux-gnueabi",
        "armv5te-none-eabi",                    // ARMv5TE A32
        "armv5te-unknown-linux-uclibceabi",     // ARMv5TE Linux with uClibc
        "armv6-unknown-freebsd",                // ARMv6 FreeBSD
        "armv6-unknown-netbsd-eabihf",
        "armv6k-nintendo-3ds",                  // ARMv6K Nintendo 3DS, Horizon (Requires devkitARM toolchain)
        "armv7-apple-ios",                      // ARMv7 iOS, Cortex-a8
        "armv7-sony-vita-newlibeabihf",         // ARM Cortex-A9 Sony PlayStation Vita (requires VITASDK toolchain)
        "armv7-unknown-linux-uclibceabi",       // ARMv7 Linux with uClibc, softfloat
        "armv7-unknown-linux-uclibceabihf",     // ARMv7 Linux with uClibc, hardfloat
        "armv7-unknown-freebsd",                // ARMv7 FreeBSD
        "armv7-unknown-netbsd-eabihf",
        "armv7-wrs-vxworks-eabihf",
        "armv7a-kmc-solid_asp3-eabi",           // ARM SOLID with TOPPERS/ASP3
        "armv7a-kmc-solid_asp3-eabihf",         // ARM SOLID with TOPPERS/ASP3, hardfloat
        "armv7a-none-eabihf",                   // ARM Cortex-A, hardfloat
        "armv7k-apple-watchos",                 // ARM Apple WatchOS
        "armv7s-apple-ios",
        "avr-unknown-gnu-atmega328",            // AVR. Requires -Z build-std=core
        "bpfeb-unknown-none",                   // BPF (big endian)
        "bpfel-unknown-none",                   // BPF (little endian)
        "hexagon-unknown-linux-musl",
        "i386-apple-ios",                       // 32-bit x86 iOS
        "i686-apple-darwin",                    // 32-bit macOS (10.7+, Lion+)
        "i686-pc-windows-msvc",                 // 32-bit Windows XP support
        "i686-unknown-haiku",                   // 32-bit Haiku
        "i686-unknown-netbsd",                  // NetBSD/i386 with SSE2
        "i686-unknown-openbsd",                 // 32-bit OpenBSD
        "i686-uwp-windows-gnu",
        "i686-uwp-windows-msvc",
        "i686-wrs-vxworks",
        "m68k-unknown-linux-gnu",               // Motorola 680x0 Linux
        "mips-unknown-linux-uclibc",            // MIPS Linux with uClibc
        "mips64-openwrt-linux-musl",            // MIPS64 for OpenWrt Linux MUSL
        "mipsel-sony-psp",                      // MIPS (LE) Sony PlayStation Portable (PSP)
        "mipsel-sony-psx",                      // MIPS (LE) Sony PlayStation 1 (PSX)
        "mipsel-unknown-linux-uclibc",          // MIPS (LE) Linux with uClibc
        "mipsel-unknown-none",                  // Bare MIPS (LE) softfloat
        "mipsisa32r6-unknown-linux-gnu",
        "mipsisa32r6el-unknown-linux-gnu",
        "mipsisa64r6-unknown-linux-gnuabi64",
        "mipsisa64r6el-unknown-linux-gnuabi64",
        "msp430-none-elf",                      // 16-bit MSP430 microcontrollers
        "powerpc-unknown-linux-gnuspe",         // PowerPC SPE Linux
        "powerpc-unknown-linux-musl",
        "powerpc-unknown-netbsd",
        "powerpc-unknown-openbsd",
        "powerpc-wrs-vxworks-spe",
        "powerpc-wrs-vxworks",
        "powerpc64-unknown-freebsd",            // PPC64 FreeBSD (ELFv1 and ELFv2)
        "powerpc64le-unknown-freebsd",          // PPC64LE FreeBSD
        "powerpc-unknown-freebsd",              // PowerPC FreeBSD
        "powerpc64-unknown-linux-musl",
        "powerpc64-wrs-vxworks",
        "powerpc64le-unknown-linux-musl",
        "powerpc64-unknown-openbsd",            // OpenBSD/powerpc64
        "powerpc64-ibm-aix",                    // 64-bit AIX (7.2 and newer)
        "riscv32gc-unknown-linux-gnu",          // RISC-V Linux (kernel 5.4, glibc 2.33)
        "riscv32gc-unknown-linux-musl",         // RISC-V Linux (kernel 5.4, musl + RISCV32 support patches)
        "riscv32im-unknown-none-elf",           // Bare RISC-V (RV32IM ISA)
        "riscv32imac-unknown-xous-elf",         // RISC-V Xous (RV32IMAC ISA)
        "riscv32imc-esp-espidf",                // RISC-V ESP-IDF
        "riscv64gc-unknown-freebsd",            // RISC-V FreeBSD
        "riscv64gc-unknown-linux-musl",         // RISC-V Linux (kernel 4.20, musl 1.2.0)
        "riscv64gc-unknown-openbsd",            // OpenBSD/riscv64
        "s390x-unknown-linux-musl",             // S390x Linux (kernel 3.2, MUSL)
        "sparc-unknown-linux-gnu",              // 32-bit SPARC Linux
        "sparc64-unknown-netbsd",               // NetBSD/sparc64
        "sparc64-unknown-openbsd",              // OpenBSD/sparc64
        "thumbv4t-none-eabi",                   // ARMv4T T32
        "thumbv5te-none-eabi",                  // ARMv5TE T32
        "thumbv7a-pc-windows-msvc",
        "thumbv7a-uwp-windows-msvc",
        "thumbv7neon-unknown-linux-musleabihf", // Thumb2-mode ARMv7a Linux with NEON, MUSL
        "wasm64-unknown-unknown",               // WebAssembly
        "x86_64-apple-ios-macabi",              // Apple Catalyst on x86_64
        "x86_64-apple-tvos",                    // x86 64-bit tvOS
        "x86_64-apple-watchos-sim",             // x86 64-bit Apple WatchOS simulator
        "x86_64-pc-nto-qnx710",                 // x86 64-bit QNX Neutrino 7.1 RTOS
        "x86_64-sun-solaris",                   // Deprecated target for 64-bit Solaris 10/11, illumos
        "x86_64-unknown-dragonfly",             // 64-bit DragonFlyBSD
        "x86_64-unknown-haiku",                 // 64-bit Haiku
        "x86_64-unknown-hermit",                // HermitCore
        "x86_64-unknown-l4re-uclibc",
        "x86_64-unknown-openbsd",               // 64-bit OpenBSD
        "x86_64-uwp-windows-gnu",
        "x86_64-uwp-windows-msvc",
        "x86_64-wrs-vxworks",

    ]);
}
