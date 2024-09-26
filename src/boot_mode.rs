//! Boot Mode
//!
//! The system boot mode indicates the "mode" in which the system is booting. The boot mode concept allows the firmware
//! to accommodate system initialization specific to a given set of circumstances represented by the boot mode. It is
//! a single value set in the HOB producer phase (e.g. PEI) and passed to the DXE phase via the Phase Handoff
//! Information Table (PHIT) HOB. During the HOB producer phase, various modules may modify the boot mode until it
//! settles upon a final value before being passed to the DXE phase.
//!
//! See <https://uefi.org/specs/PI/1.8A/V1_Boot_Paths.html#defined-boot-modes>
//! See <https://uefi.org/specs/PI/1.8A/V3_HOB_Code_Definitions.html#efi-hob-handoff-info-table-phit-hob>
//!
//! ## Example
//! ```
//! use mu_pi::BootMode;
//!
//! let boot_mode = BootMode::BootWithFullConfiguration;
//! println!("Boot Mode: {}", boot_mode);
//!
//! ## License
//!
//! Copyright (c) Microsoft Corporation
//!
//! SPDX-License-Identifier: BSD-2-Clause-Patent
//!

use core::fmt;

// All targets currently assume that that the boot mode is represented as a u32
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Mode {
    /// The basic S0 boot path. Informs all PEIMs to do a full configuration. The basic S0 boot path must be supported.
    BootWithFullConfiguration,
    /// A variation on the basic S0 boot path. Indicates that the minimal amount of hardware should be initialized
    /// to boot the system.
    BootWithMinimalConfiguration,
    /// A variation on the basic S0 boot path. Indicates that the configuration data from the last boot should be used
    /// without any changes.
    BootAssumingNoConfigurationChanges,
    /// A variation on the basic S0 boot path. Indicates that the any diagnostic code should be run.
    BootWithFullConfigurationPlusDiagnostic,
    /// A variation on the basic S0 boot path. Indicates that a known set of safe values for programming hardware
    /// should be used.
    BootWithDefaultSettings,
    /// The current boot is a S4 (Save to Disk) hibernate resume.
    BootOnS4Resume,
    /// The current boot is a S5 (Soft Off) power on. Some platforms may use this mode to differentiate between a normal
    /// for example, if buttons other than the power button can wake the system.
    BootOnS5Resume,
    /// The current boot is a manufacturing mode boot. Firmware drivers may parameterize actions based on this mode
    /// that should only occur in a factory or a manufacturing environment.
    BootWithMfgModeSettings,
    /// The boot is a S2 (Sleep) resume boot.
    BootOnS2Resume = 0x10,
    /// The boot is a S3 (Save to RAM) resume boot. Platforms that support S3 resume must take special care to
    /// preserve/restore memory and critical hardware
    BootOnS3Resume,
    /// This boot mode can be either an INIT, S3, or other means by which to restart the machine. If it is an
    /// S3, for example, the flash update cause will supersede the S3 restart. It is incumbent upon platform
    /// code, such as the Memory Initialization PEIM, to determine the exact cause and perform correct behavior
    /// (i.e., S3 state restoration versus INIT behavior).
    BootOnFlashUpdate,
    /// The boot is in recovery mode. This mode is used to recover from a previous boot failure.
    BootInRecoveryMode = 0x20,
}

// Implement Display for Mode to output a string for each enum variant
impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} ({:#x?})",
            match self {
                Mode::BootWithFullConfiguration => "Boot With Full Configuration",
                Mode::BootWithMinimalConfiguration => "Boot With Minimal Configuration",
                Mode::BootAssumingNoConfigurationChanges => "Boot Assuming No Configuration Changes",
                Mode::BootWithFullConfigurationPlusDiagnostic => "Boot With Full Configuration Plus Diagnostic",
                Mode::BootWithDefaultSettings => "Boot With Default Settings",
                Mode::BootOnS4Resume => "Boot On S4 Resume",
                Mode::BootOnS5Resume => "Boot On S5 Resume",
                Mode::BootWithMfgModeSettings => "Boot With Mfg Mode Settings",
                Mode::BootOnS2Resume => "Boot On S2 Resume",
                Mode::BootOnS3Resume => "Boot On S3 Resume",
                Mode::BootOnFlashUpdate => "Boot On Flash Update",
                Mode::BootInRecoveryMode => "Boot In Recovery Mode",
            },
            *self as u32
        )
    }
}
