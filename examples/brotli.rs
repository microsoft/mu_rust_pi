extern crate mu_pi;
use alloc_no_stdlib::{self, define_index_ops_mut, SliceWrapper, SliceWrapperMut};
use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, HuffmanCode};
use mu_pi::fw_fs::{
  ffs::{FfsSectionIterator, Section, SectionExtractor, SectionMetaData},
  FirmwareVolume,
};
use r_efi::efi;
use std::{env, error::Error, fs, path::Path};

//Rebox and HeapAllocator satisfy BrotliDecompress custom allocation requirement.
struct Rebox<T>(Box<[T]>);

impl<T> core::default::Default for Rebox<T> {
  fn default() -> Self {
    Rebox(Vec::new().into_boxed_slice())
  }
}
define_index_ops_mut!(T, Rebox<T>);

impl<T> alloc_no_stdlib::SliceWrapper<T> for Rebox<T> {
  fn slice(&self) -> &[T] {
    &self.0
  }
}

impl<T> alloc_no_stdlib::SliceWrapperMut<T> for Rebox<T> {
  fn slice_mut(&mut self) -> &mut [T] {
    &mut self.0
  }
}

struct HeapAllocator<T: Clone> {
  pub default_value: T,
}

impl<T: Clone> alloc_no_stdlib::Allocator<T> for HeapAllocator<T> {
  type AllocatedMemory = Rebox<T>;
  fn alloc_cell(self: &mut HeapAllocator<T>, len: usize) -> Rebox<T> {
    Rebox(vec![self.default_value.clone(); len].into_boxed_slice())
  }
  fn free_cell(self: &mut HeapAllocator<T>, _data: Rebox<T>) {}
}

pub const BROTLI_SECTION_GUID: efi::Guid =
  efi::Guid::from_fields(0x3D532050, 0x5CDA, 0x4FD0, 0x87, 0x9E, &[0x0F, 0x7F, 0x63, 0x0D, 0x5A, 0xFB]);

#[derive(Debug, Clone, Copy)]
struct BrotliSectionExtractor {}

impl SectionExtractor for BrotliSectionExtractor {
  fn extract(&self, section: Section) -> Vec<Section> {
    if let SectionMetaData::GuidDefined(meta_data) = section.metadata() {
      if meta_data.section_definition_guid == BROTLI_SECTION_GUID {
        let data = section.section_data();
        let out_size = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let _scratch_size = u64::from_le_bytes(data[8..16].try_into().unwrap());

        let mut brotli_state = BrotliState::new(
          HeapAllocator::<u8> { default_value: 0 },
          HeapAllocator::<u32> { default_value: 0 },
          HeapAllocator::<HuffmanCode> { default_value: Default::default() },
        );
        let in_data = &data[16..];
        let mut out_data = vec![0u8; out_size as usize];
        let mut out_data_size = 0;
        let result = BrotliDecompressStream(
          &mut in_data.len(),
          &mut 0,
          &data[16..],
          &mut out_data.len(),
          &mut 0,
          out_data.as_mut_slice(),
          &mut out_data_size,
          &mut brotli_state,
        );

        if matches!(result, BrotliResult::ResultSuccess) {
          // deliberate leak - memory must remain valid for 'static since Section instances it produces use &'static
          // references to it.
          let out_buffer_ptr = Box::into_raw(out_data.into_boxed_slice());
          let out_buffer_static_ref = unsafe { out_buffer_ptr.as_ref().unwrap() };
          if let Ok(first_encapsulated_section) = unsafe {
            Section::new_in_extraction_buffer(
              section.containing_file(),
              out_buffer_ptr as *const u8 as efi::PhysicalAddress,
              out_buffer_static_ref,
            )
          } {
            return FfsSectionIterator::new_with_extractor(Some(first_encapsulated_section), Box::new(*self)).collect();
          }
        }
      }
    }
    Vec::new()
  }
}

fn print_fv(fv: FirmwareVolume) {
  println!("Firmware Volume:");
  for ffs_file in fv.ffs_files() {
    println!("  file: {:x?}", ffs_file);
    for section in ffs_file.ffs_sections_with_extractor(Box::new(BrotliSectionExtractor {})) {
      println!("    section: {:x?}", section);
    }
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  let root = Path::new(&env::var("CARGO_MANIFEST_DIR")?).join("test_resources");

  let fv_bytes = fs::read(root.join("FVMAIN_COMPACT.Fv"))?;
  let fv = unsafe { FirmwareVolume::new(fv_bytes.as_ptr() as efi::PhysicalAddress).unwrap() };

  print_fv(fv);
  Ok(())
}
