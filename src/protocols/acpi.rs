use uefi_sdk::boot_services::StandardBootServices;
use uefi_sdk::component::{service::IntoService, IntoComponent};
use uefi_sdk::services::acpi::{AcpiTable, AcpiTableError, AcpiTableManager, TableKey};

#[derive(IntoService)]
#[service(dyn AcpiTableManager)]
pub struct CoreAcpiTableManager;

impl AcpiTableManager for CoreAcpiTableManager {
    fn install_acpi_table(&self, _acpi_table: &AcpiTable) -> Result<TableKey, AcpiTableError> {
        todo!()
    }

    fn uninstall_acpi_table(&self, _table_key: TableKey) -> Result<(), AcpiTableError> {
        todo!()
    }

    fn get_acpi_table(&self, _index: usize) -> Result<&AcpiTable, AcpiTableError> {
        todo!()
    }
}

#[derive(IntoComponent)]
pub struct AcpiTableComponent {}

impl AcpiTableComponent {
    fn entry_point(self, bs: StandardBootServices) -> uefi_sdk::error::Result<()> {
        Ok(())
    }
}
