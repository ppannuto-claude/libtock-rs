use super::Cli;
use std::env::{var, VarError};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

// The QEMU binary to run. libtock-rs uses whatever QEMU is installed on the
// system rather than building its own; `LIBTOCK_QEMU` overrides which binary
// that is, for people who have a QEMU build somewhere other than their PATH.
const DEFAULT_QEMU: &str = "qemu-system-riscv32";
const QEMU_VAR: &str = "LIBTOCK_QEMU";

fn qemu_binary() -> String {
    match var(QEMU_VAR) {
        Ok(binary) => binary,
        Err(VarError::NotPresent) => DEFAULT_QEMU.into(),
        Err(VarError::NotUnicode(binary)) => {
            panic!("Non-UTF-8 {QEMU_VAR} value: {binary:?}")
        }
    }
}

// Spawns a QEMU VM with a simulated Tock system and the process binary. Returns
// the handle for the spawned QEMU process.
pub fn deploy(cli: &Cli, platform: String, tbf_path: PathBuf) -> Child {
    let platform_args = get_platform_args(platform);
    let device = format!(
        "loader,file={},addr={}",
        tbf_path
            .into_os_string()
            .into_string()
            .expect("Non-UTF-8 path"),
        platform_args.process_binary_load_address,
    );
    let binary = qemu_binary();
    let mut qemu = Command::new(&binary);
    qemu.args(["-device", &device, "-nographic", "-serial", "mon:stdio"]);
    qemu.args(platform_args.fixed_args);
    // If we let QEMU inherit its stdin from us, it will set it to raw mode,
    // which prevents Ctrl+C from generating SIGINT. QEMU will not exit when
    // Ctrl+C is entered, making our runner hard to close. Instead, we forward
    // stdin to QEMU ourselves -- see output_processor.rs for more details.
    qemu.stdin(Stdio::piped());
    qemu.stdout(Stdio::piped());
    // Because we set the terminal to raw mode while running QEMU, but QEMU's
    // stdin is not connected to a terminal, QEMU does not know it needs to use
    // CRLF line endings when printing to stderr. To convert, we also pipe
    // QEMU's stderr through us and output_processor converts the line endings.
    qemu.stderr(Stdio::piped());
    if cli.verbose {
        println!("QEMU command: {qemu:?}");
        println!("Spawning QEMU")
    }
    qemu.spawn().unwrap_or_else(|error| {
        panic!(
            "failed to spawn QEMU ({binary}): {error}\n\
             Install a QEMU with 32-bit RISC-V support (the qemu-system-misc package on \
             Debian and Ubuntu, qemu-system-riscv on Fedora, or qemu on Homebrew), or point \
             {QEMU_VAR} at a qemu-system-riscv32 binary."
        )
    })
}

// Returns the command line arguments for the given platform to qemu. Panics if
// an unknown platform is passed.
fn get_platform_args(platform: String) -> PlatformConfig {
    match platform.as_str() {
        "hifive1" => PlatformConfig {
            #[rustfmt::skip]
            fixed_args: &[
                "-kernel", "tock/target/riscv32imac-unknown-none-elf/release/hifive1",
                "-M", "sifive_e,revb=true",
            ],
            process_binary_load_address: "0x20040000",
        },
        "opentitan" => PlatformConfig {
            // The earlgrey-cw310 kernel is linked to start at ORIGIN(rom) plus
            // the size of the manifest, so QEMU has to be told to reset there
            // rather than at the start of the ROM. These arguments mirror the
            // `qemu` target in Tock's boards/opentitan/earlgrey-cw310/Makefile.
            // They replace a `-bios tock/tools/qemu-runner/opentitan-boot-rom.elf`
            // argument that referred to a file which no longer exists in Tock.
            #[rustfmt::skip]
            fixed_args: &[
                "-kernel", "tock/target/riscv32imc-unknown-none-elf/release/earlgrey-cw310",
                "-M", "opentitan",
                "-global", "driver=riscv.lowrisc.ibex.soc,property=resetvec,value=0x20000400",
            ],
            process_binary_load_address: "0x20030000",
        },
        _ => panic!("Cannot deploy to platform {platform} via QEMU."),
    }
}

// QEMU configuration information that is specific to each platform.
struct PlatformConfig {
    fixed_args: &'static [&'static str],
    process_binary_load_address: &'static str,
}
