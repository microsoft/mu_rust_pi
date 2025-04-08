use core::{
    cell::OnceCell,
    ffi::c_void,
    ptr::{null, null_mut},
};
use r_efi::efi;
use spin::Once;
use uefi_sdk::{
    boot_services::StandardBootServices,
    component::{
        service::{IntoService, Service},
        IntoComponent,
    },
    services::acpi::{AcpiSdtManager, AcpiTable, AcpiTableError, AcpiTableManager, TableKey},
};

// struct can be component and service
// register service with core w/o component

#[derive(IntoService)]
#[service(dyn AcpiTableManager)]
#[protocol = "ffe06bdd-6107-46a6-7bb2-5a9c7ec5275c"]
#[repr(C)]
pub struct CoreAcpiTableManager {
    pub acpi_protocol: AcpiTableProtocol,
    pub state: AcpiTableProtocolSupport,
}

#[derive(IntoService)]
#[service(dyn AcpiSdtManager)]
#[protocol = "eb97088e-cfdf-49c6-be4b-d906a5b20e86"]
#[repr(C)]
pub struct CoreAcpiSdtManager {
    pub sdt_protocol: AcpiSdtProtocol,
    pub state: AcpiTableProtocolSupport,
}

impl CoreAcpiTableManager {
    extern "efiapi" fn install_acpi_table_ext(
        protocol: *const AcpiTableProtocol,
        acpi_table_buffer: *const c_void,
        acpi_table_buffer_size: usize,
        table_key: *mut usize,
    ) -> efi::Status {
        let this = protocol as *const CoreAcpiTableManager;
        // this unsafeness can be cleaned up a little
        unsafe {
            this.as_ref().unwrap().install_acpi_table(&*(acpi_table_buffer as *const AcpiTable));
        }
        efi::Status::SUCCESS
    }

    extern "efiapi" fn uninstall_acpi_table_ext(protocol: *const AcpiTableProtocol, table_key: usize) -> efi::Status {
        let this = protocol as *const CoreAcpiTableManager;
        unsafe { this.as_ref().unwrap().uninstall_acpi_table(table_key) };
        efi::Status::SUCCESS
    }
}

impl AcpiTableManager for CoreAcpiTableManager {
    fn install_acpi_table(&self, _acpi_table: &AcpiTable) -> Result<TableKey, AcpiTableError> {
        todo!()
    }

    fn uninstall_acpi_table(&self, _table_key: TableKey) -> Result<(), AcpiTableError> {
        todo!()
    }
}

impl AcpiSdtManager for CoreAcpiSdtManager {
    fn get_acpi_table(&self, index: usize) -> Result<&AcpiTable, AcpiTableError> {
        // unfortunately i think this will have to do the "worse" option
        // which is wrapping the C function (vs. having the C function call into a rust function)
        // between C wrapping rust and rust wrapping C i think the former is superior
        // but i can't think of an easy and clean way for get_acpi_table_ext to call into get_acpi_table
        // even with the global support variable
        // without lots of self-referential messes
        CoreAcpiSdtManager::get_acpi_table_ext(index, null_mut(), null_mut(), null_mut());
        Err(AcpiTableError::GenericError)
    }
}

impl CoreAcpiSdtManager {
    extern "efiapi" fn get_acpi_table_ext(
        index: usize,
        table: *mut *mut AcpiSdtHeader,
        version: *mut u32,
        table_key: *mut usize,
    ) -> efi::Status {
        // the following line is just a reminder that ACPI_TABLE_INFO is used by this function
        let ops = &ACPI_TABLE_INFO.get();
        efi::Status::SUCCESS
    }
}

// this is BAD (global static)
// but feels unavoidable due to the interface of get_acpi_table in C
// which operates with a "stateless" interface
// while referencing a global EFI_ACPI_TABLE_INSTANCE :(
pub static ACPI_TABLE_INFO: Once<&'static AcpiTableProtocolSupport> = Once::new();

// goal is for this to install the protocol/services
#[derive(IntoComponent)]
pub struct AcpiTableComponent {}

impl AcpiTableComponent {
    fn entry_point(self, bs: StandardBootServices) -> uefi_sdk::error::Result<()> {
        // register support service
        // register table + sdt services (which also installs protocols)
        initialize_acpi_table();
        Ok(())
    }
}

struct AcpiTableProtocolSupport {
    pub signature: usize,
    // i haven't written out the rest of the fields but
    // it ideally should mirror EFI_ACPI_TABLE_INSTANCE
}

#[repr(C)]
struct AcpiSdtHeader {
    signature: u32,
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

type AcpiTableInstall = extern "efiapi" fn(*const AcpiTableProtocol, *const c_void, usize, *mut usize) -> efi::Status;
type AcpiTableUninstall = extern "efiapi" fn(*const AcpiTableProtocol, usize) -> efi::Status;
type AcpiTableGet = extern "efiapi" fn(usize, *mut *mut AcpiSdtHeader, *mut u32, *mut usize) -> efi::Status;

// is this right for publishing in C? idk
#[repr(C)]
struct AcpiTableProtocol {
    install_table: AcpiTableInstall,
    uninstall_table: AcpiTableUninstall,
}

impl AcpiTableProtocol {
    fn new() -> Self {
        Self {
            install_table: CoreAcpiTableManager::install_acpi_table_ext,
            uninstall_table: CoreAcpiTableManager::uninstall_acpi_table_ext,
        }
    }
}

#[repr(C)]
struct AcpiSdtProtocol {
    get_table: AcpiTableGet,
}

impl AcpiSdtProtocol {
    fn new() -> Self {
        Self { get_table: CoreAcpiSdtManager::get_acpi_table_ext }
    }
}

// maybe this would make more sense in uefi-dxe-core
// or maybe ALL this code should be in uefi-dxe-core
// also in C, this has 2 arguments (system table, image handle)
// but these aren't actually used by the init
// so i'm guessing it's just templated C that we can ignore
pub fn initialize_acpi_table() {
    // do initialization of ACPI_TABLE_INFO
}
