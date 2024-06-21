//! Firmware File System (FFS) Definitions and Support Code
//!
//! Based on the values defined in the UEFI Platform Initialization (PI) Specification V1.8A Section 3.2.2
//! Firmware File System.
//!
//! ## License
//!
//! Copyright (C) Microsoft Corporation. All rights reserved.
//!
//! SPDX-License-Identifier: BSD-2-Clause-Patent
//!

extern crate alloc;

pub mod attributes;
pub mod file;
pub mod guid;
pub mod section;

use core::{fmt, mem, slice};

use alloc::{boxed::Box, collections::VecDeque, vec::Vec};
use attributes::raw::LARGE_FILE;
use r_efi::efi;
use section::header::{CommonSectionHeaderExtended, CommonSectionHeaderStandard};
use uuid::Uuid;

use crate::{
  address_helper::align_up,
  fw_fs::{
    ffs::{
      attributes::raw as EfiFfsFileAttributeRaw,
      file::{raw::r#type as FfsFileRawType, Type as FfsFileType},
      section as FfsSection,
      section::{header as FfsSectionHeader, raw_type as FfsSectionRawType},
    },
    fv::{
      file::{raw::attribute as FvRawAttribute, EfiFvFileAttributes},
      FirmwareVolume, Header as FvHeader,
    },
    fvb::attributes::raw::fvb2 as Fvb2RawAttributes,
  },
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum FfsFileHeader {
  Standard(&'static file::Header),
  Extended(&'static file::Header2),
}

impl FfsFileHeader {
  fn header(&self) -> &'static file::Header {
    match self {
      Self::Standard(header) => header,
      Self::Extended(header) => &header.header,
    }
  }

  fn size(&self) -> u64 {
    match self {
      Self::Standard(header) => {
        let mut size: u64 = 0;
        size += header.size[0] as u64;
        size += (header.size[1] as u64) << 8;
        size += (header.size[2] as u64) << 16;
        size
      }
      Self::Extended(header) => header.extended_size,
    }
  }

  fn data_offset(&self) -> usize {
    match self {
      Self::Standard(_) => mem::size_of::<file::Header>(),
      Self::Extended(_) => mem::size_of::<file::Header2>(),
    }
  }

  fn base_address(&self) -> efi::PhysicalAddress {
    match self {
      Self::Standard(header) => *header as *const file::Header as efi::PhysicalAddress,
      Self::Extended(header2) => *header2 as *const file::Header2 as efi::PhysicalAddress,
    }
  }

  fn data_address(&self) -> efi::PhysicalAddress {
    self.base_address() + self.data_offset() as u64
  }
}

impl From<&'static file::Header> for FfsFileHeader {
  fn from(file: &'static file::Header) -> Self {
    if (file.attributes & LARGE_FILE) != 0 {
      let extended = unsafe { (file as *const file::Header as *const file::Header2).as_ref().unwrap() };
      Self::Extended(extended)
    } else {
      Self::Standard(file)
    }
  }
}

/// Firmware File System (FFS) File.
#[derive(Copy, Clone)]
pub struct File {
  containing_fv: FirmwareVolume,
  ffs_file: FfsFileHeader,
}

impl File {
  /// Instantiate a new File structure given the containing volume and base address.
  ///
  /// ## Safety
  /// Caller must ensure that base_address points to the start of a valid FFS header and that it is safe to access
  /// memory from the start of that header to the full length fo the file specified by that header. Caller must also
  /// ensure that the memory containing the file data outlives this File instance.
  ///
  /// Various sanity checks will be performed by this routine to ensure File consistency.
  pub unsafe fn new(
    containing_fv: FirmwareVolume,
    file_base_address: efi::PhysicalAddress,
  ) -> Result<File, efi::Status> {
    if file_base_address < containing_fv.base_address() || containing_fv.top_address() <= file_base_address {
      Err(efi::Status::INVALID_PARAMETER)?;
    }

    let ffs_file = file_base_address as *const file::Header;
    let ffs_file = ffs_file.as_ref().ok_or(efi::Status::INVALID_PARAMETER)?;

    let ffs_file = ffs_file.into();

    Ok(File { containing_fv, ffs_file })
  }

  /// Returns the file size (including header).
  pub fn file_size(&self) -> u64 {
    self.ffs_file.size()
  }

  /// Returns file data size (not including header).
  pub fn file_data_size(&self) -> u64 {
    self.ffs_file.size() - self.ffs_file.data_offset() as u64
  }

  /// Returns the file type.
  pub fn file_type(&self) -> Option<FfsFileType> {
    match self.ffs_file.header().file_type {
      FfsFileRawType::RAW => Some(FfsFileType::Raw),
      FfsFileRawType::FREEFORM => Some(FfsFileType::FreeForm),
      FfsFileRawType::SECURITY_CORE => Some(FfsFileType::SecurityCore),
      FfsFileRawType::PEI_CORE => Some(FfsFileType::PeiCore),
      FfsFileRawType::DXE_CORE => Some(FfsFileType::DxeCore),
      FfsFileRawType::PEIM => Some(FfsFileType::Peim),
      FfsFileRawType::DRIVER => Some(FfsFileType::Driver),
      FfsFileRawType::COMBINED_PEIM_DRIVER => Some(FfsFileType::CombinedPeimDriver),
      FfsFileRawType::APPLICATION => Some(FfsFileType::Application),
      FfsFileRawType::MM => Some(FfsFileType::Mm),
      FfsFileRawType::FIRMWARE_VOLUME_IMAGE => Some(FfsFileType::FirmwareVolumeImage),
      FfsFileRawType::COMBINED_MM_DXE => Some(FfsFileType::CombinedMmDxe),
      FfsFileRawType::MM_CORE => Some(FfsFileType::MmCore),
      FfsFileRawType::MM_STANDALONE => Some(FfsFileType::MmStandalone),
      FfsFileRawType::MM_CORE_STANDALONE => Some(FfsFileType::MmCoreStandalone),
      FfsFileRawType::OEM_MIN..=FfsFileRawType::OEM_MAX => Some(FfsFileType::OemMin),
      FfsFileRawType::DEBUG_MIN..=FfsFileRawType::DEBUG_MAX => Some(FfsFileType::DebugMin),
      FfsFileRawType::FFS_PAD => Some(FfsFileType::FfsPad),
      FfsFileRawType::FFS_MIN..=FfsFileRawType::FFS_MAX => Some(FfsFileType::FfsUnknown),
      _ => None,
    }
  }

  /// Returns the FV File Attributes (see PI spec 1.8A 3.4.1.4).
  pub fn fv_file_attributes(&self) -> EfiFvFileAttributes {
    let attributes = self.ffs_file.header().attributes;
    let data_alignment = (attributes & EfiFfsFileAttributeRaw::DATA_ALIGNMENT) >> 3;
    // decode alignment per Table 3.3 in PI spec 1.8 Part III.
    let mut file_attributes: u32 = match (
      data_alignment,
      (attributes & EfiFfsFileAttributeRaw::DATA_ALIGNMENT_2) == EfiFfsFileAttributeRaw::DATA_ALIGNMENT_2,
    ) {
      (0, false) => 0,
      (1, false) => 4,
      (2, false) => 7,
      (3, false) => 9,
      (4, false) => 10,
      (5, false) => 12,
      (6, false) => 15,
      (7, false) => 16,
      (x @ 0..=7, true) => (17 + x) as u32,
      (_, _) => panic!("Invalid data_alignment!"),
    };
    if attributes & EfiFfsFileAttributeRaw::FIXED != 0 {
      file_attributes |= FvRawAttribute::FIXED;
    }
    file_attributes as EfiFvFileAttributes
  }

  /// Returns the GUID filename for this file.
  pub fn file_name(&self) -> efi::Guid {
    self.ffs_file.header().name
  }

  /// Returns the base address in memory of this file.
  pub fn base_address(&self) -> efi::PhysicalAddress {
    self.ffs_file.base_address()
  }

  /// Returns the memory address of the end of the file (not inclusive).
  pub fn top_address(&self) -> efi::PhysicalAddress {
    self.base_address() + self.file_size()
  }

  /// Returns the file data.
  pub fn file_data(&self) -> &[u8] {
    let data_ptr = self.ffs_file.data_address() as *mut u8;
    unsafe { slice::from_raw_parts(data_ptr, self.file_data_size() as usize) }
  }

  /// Returns the next file in the Firmware Volume, if any.
  pub fn next_ffs_file(&self) -> Option<File> {
    let mut next_file_address = self.base_address();
    next_file_address += self.file_size();

    // per the PI spec, "Given a file F, the next file FvHeader is located at the next 8-byte aligned firmware volume
    // offset following the last byte the file F"
    // but, in fact, that just means "8-byte aligned" per the EDK2 implementation.
    next_file_address = align_up(next_file_address, 0x8);

    // check to see if we ran off the end of the FV yet.
    let erase_byte: [u8; 1] =
      if self.containing_fv.attributes() & Fvb2RawAttributes::ERASE_POLARITY != 0 { [0xFF] } else { [0] };
    let remaining_space = self.containing_fv.top_address() - mem::size_of::<FvHeader>() as efi::PhysicalAddress;
    if next_file_address <= remaining_space {
      let test_size = mem::size_of::<FvHeader>().min(remaining_space.try_into().unwrap());
      let remaining_space_slice = unsafe { slice::from_raw_parts(next_file_address as *const u8, test_size) };

      if remaining_space_slice.windows(mem::size_of_val(&erase_byte)).all(|window| window == erase_byte) {
        // No files are left, only erased bytes
        return None;
      }

      // validation of the file is performed in the File::new constructor.
      unsafe { File::new(self.containing_fv, next_file_address).ok() }
    } else {
      None
    }
  }

  /// Returns the first section of the file, if any.
  pub fn first_ffs_section(&self) -> Option<Section> {
    if self.file_size() <= self.ffs_file.data_offset() as u64 {
      return None;
    }
    let first_section = unsafe { Section::new(*self, self.ffs_file.data_address()).ok()? };
    Some(first_section)
  }

  /// Returns an iterator over the sections of the file.
  pub fn ffs_sections(&self) -> impl Iterator<Item = Section> {
    FfsSectionIterator::new(self.first_ffs_section())
  }

  /// Returns an iterator over the sections of the file, using the provided section extractor.
  pub fn ffs_sections_with_extractor(&self, extractor: Box<dyn SectionExtractor>) -> impl Iterator<Item = Section> {
    FfsSectionIterator::new_with_extractor(self.first_ffs_section(), extractor)
  }

  /// Returns the raw file type.
  pub fn file_type_raw(&self) -> u8 {
    self.ffs_file.header().file_type
  }

  /// Returns the raw file attributes.
  pub fn file_attributes_raw(&self) -> u8 {
    self.ffs_file.header().attributes
  }

  /// Returns the base address of the containing FV.
  pub fn containing_fv_base(&self) -> efi::PhysicalAddress {
    self.containing_fv.base_address()
  }
}

impl fmt::Debug for File {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "File @{:#x} type: {:?} name: {:?} size: {:?}",
      self.base_address(),
      self.file_type(),
      Uuid::from_bytes_le(*self.file_name().as_bytes()),
      self.file_size()
    )
  }
}

pub(crate) struct FileIterator {
  next_ffs: Option<File>,
}

impl FileIterator {
  pub fn new(start_file: Option<File>) -> FileIterator {
    FileIterator { next_ffs: start_file }
  }
}

impl Iterator for FileIterator {
  type Item = File;
  fn next(&mut self) -> Option<File> {
    let current = self.next_ffs?;
    self.next_ffs = current.next_ffs_file();
    Some(current)
  }
}

/// A section extractor that can be passed to [`FfsSectionIterator`] to unpack encapsulated sections.
pub trait SectionExtractor {
  /// Extract the given encapsulated section and return the contained sections as a vector.
  fn extract(&self, section: Section) -> Vec<Section>;
}

#[derive(Debug, Clone, Copy)]
enum CommonSectionHeader {
  Standard(&'static FfsSectionHeader::CommonSectionHeaderStandard),
  Extended(&'static FfsSectionHeader::CommonSectionHeaderExtended),
}

impl CommonSectionHeader {
  unsafe fn new(base_address: efi::PhysicalAddress) -> Result<CommonSectionHeader, ()> {
    let common_hdr_ptr = (base_address as *const FfsSectionHeader::CommonSectionHeaderStandard).as_ref().ok_or(())?;

    let size = &common_hdr_ptr.size;

    if size.iter().all(|x| *x == 0xff) {
      let extended_hdr_ptr =
        (base_address as *const FfsSectionHeader::CommonSectionHeaderExtended).as_ref().ok_or(())?;
      Ok(CommonSectionHeader::Extended(extended_hdr_ptr))
    } else {
      Ok(CommonSectionHeader::Standard(common_hdr_ptr))
    }
  }

  fn section_type(&self) -> u8 {
    match self {
      CommonSectionHeader::Standard(header) => header.section_type,
      CommonSectionHeader::Extended(header) => header.section_type,
    }
  }

  fn section_size(&self) -> usize {
    match self {
      CommonSectionHeader::Standard(header) => {
        let mut size_bytes = header.size.to_vec();
        size_bytes.push(0);
        let size: u32 = u32::from_le_bytes(size_bytes.try_into().unwrap());
        size as usize
      }
      CommonSectionHeader::Extended(header) => header.extended_size as usize,
    }
  }

  fn base_address(&self) -> efi::PhysicalAddress {
    match *self {
      CommonSectionHeader::Standard(header) => header as *const _ as efi::PhysicalAddress,
      CommonSectionHeader::Extended(header) => header as *const _ as efi::PhysicalAddress,
    }
  }

  fn header_size(&self) -> usize {
    match self {
      CommonSectionHeader::Standard(_) => mem::size_of::<CommonSectionHeaderStandard>(),
      CommonSectionHeader::Extended(_) => mem::size_of::<CommonSectionHeaderExtended>(),
    }
  }
}

/// Section metadata
#[derive(Debug, Clone, Copy)]
pub enum SectionMetaData {
  None,
  Compression(&'static FfsSectionHeader::Compression),
  GuidDefined(&'static FfsSectionHeader::GuidDefined),
  Version(&'static FfsSectionHeader::Version),
  FreeformSubtypeGuid(&'static FfsSectionHeader::FreeformSubtypeGuid),
}

/// Firmware File System (FFS) Section.
#[derive(Clone, Copy)]
pub struct Section {
  containing_ffs: File,
  containing_extraction_buffer: Option<&'static [u8]>,
  header: CommonSectionHeader,
  meta_data: SectionMetaData,
  data: &'static [u8],
}

impl Section {
  /// Instantiate a new Section structure given the containing file and base address.
  ///
  /// ## Safety
  /// Caller must ensure that base_address points to the start of a valid FFS section header and that it is safe to
  /// access memory from the start of that header to the full length fo the section specified by that header. Caller
  /// must also ensure that the memory containing the section data outlives this Section instance.
  ///
  /// Various sanity checks will be performed by this routine to ensure Section consistency.
  pub unsafe fn new(containing_ffs: File, base_address: efi::PhysicalAddress) -> Result<Section, efi::Status> {
    let header = CommonSectionHeader::new(base_address).map_err(|_| efi::Status::INVALID_PARAMETER)?;

    let (meta_data, data, len) = match header.section_type() {
      FfsSectionRawType::encapsulated::COMPRESSION => {
        let compression = (header.base_address() + header.header_size() as efi::PhysicalAddress)
          as *const FfsSectionHeader::Compression;
        let compression = unsafe { compression.as_ref().ok_or(efi::Status::INVALID_PARAMETER)? };
        let total_header = header.header_size() + mem::size_of::<FfsSectionHeader::Compression>();
        let data = (header.base_address() + total_header as efi::PhysicalAddress) as *const u8;
        let len = header.section_size() - total_header;
        (SectionMetaData::Compression(compression), data, len)
      }
      FfsSectionRawType::encapsulated::GUID_DEFINED => {
        let guid_defined = (header.base_address() + header.header_size() as efi::PhysicalAddress)
          as *const FfsSectionHeader::GuidDefined;
        let guid_defined = unsafe { guid_defined.as_ref().ok_or(efi::Status::INVALID_PARAMETER)? };
        let total_header = header.header_size() + mem::size_of::<FfsSectionHeader::GuidDefined>();
        let data = (header.base_address() + total_header as efi::PhysicalAddress) as *const u8;
        let len = header.section_size() - total_header;
        (SectionMetaData::GuidDefined(guid_defined), data, len)
      }
      FfsSectionRawType::VERSION => {
        let version =
          (header.base_address() + header.header_size() as efi::PhysicalAddress) as *const FfsSectionHeader::Version;
        let version = unsafe { version.as_ref().ok_or(efi::Status::INVALID_PARAMETER)? };
        let total_header = header.header_size() + mem::size_of::<FfsSectionHeader::Version>();
        let data = (header.base_address() + total_header as efi::PhysicalAddress) as *const u8;
        let len = header.section_size() - total_header;
        (SectionMetaData::Version(version), data, len)
      }
      FfsSectionRawType::FREEFORM_SUBTYPE_GUID => {
        let freeform_subtype = (header.base_address() + header.header_size() as efi::PhysicalAddress)
          as *const FfsSectionHeader::FreeformSubtypeGuid;
        let freeform_subtype = unsafe { freeform_subtype.as_ref().ok_or(efi::Status::INVALID_PARAMETER)? };
        let total_header = header.header_size() + mem::size_of::<FfsSectionHeader::FreeformSubtypeGuid>();
        let data = (header.base_address() + total_header as efi::PhysicalAddress) as *const u8;
        let len = header.section_size() - total_header;
        (SectionMetaData::FreeformSubtypeGuid(freeform_subtype), data, len)
      }
      _ => {
        let data = (header.base_address() + header.header_size() as efi::PhysicalAddress) as *const u8;
        let len = header.section_size() - header.header_size();
        (SectionMetaData::None, data, len)
      }
    };

    let data = unsafe { slice::from_raw_parts(data, len) };

    Ok(Section { containing_ffs, containing_extraction_buffer: None, header, meta_data, data })
  }

  /// Instantiate a new Section structure given the containing file and base address that is part of an extracted
  /// section.
  ///
  /// ## Safety
  /// Caller must ensure that base_address points to the start of a valid FFS section header and that it is safe to
  /// access memory from the start of that header to the full length fo the section specified by that header. Caller
  /// must also ensure that the memory containing the section data outlives this Section instance.
  ///
  /// Various sanity checks will be performed by this routine to ensure Section consistency.
  pub unsafe fn new_in_extraction_buffer(
    containing_ffs: File,
    base_address: efi::PhysicalAddress,
    extraction_buffer: &'static [u8],
  ) -> Result<Section, efi::Status> {
    let mut section = Self::new(containing_ffs, base_address)?;
    section.containing_extraction_buffer = Some(extraction_buffer);
    Ok(section)
  }

  /// Returns the base address in memory of this section.
  ///
  /// Note: if this is a section contained within an encapsulation section, then the base address of this section is not
  /// guaranteed to be within the bounds of the containing [`File`]'s data buffer.
  pub fn base_address(&self) -> efi::PhysicalAddress {
    self.header.base_address()
  }

  #[cfg(test)]
  pub fn container_offset(&self) -> usize {
    if let Some(container_buffer) = self.containing_extraction_buffer {
      (self.base_address() - container_buffer.as_ptr() as efi::PhysicalAddress) as usize
    } else {
      (self.base_address() - self.containing_ffs.containing_fv_base()) as usize
    }
  }

  /// Returns the section type.
  pub fn section_type(&self) -> Option<FfsSection::Type> {
    match self.header.section_type() {
      FfsSectionRawType::encapsulated::COMPRESSION => Some(FfsSection::Type::Compression),
      FfsSectionRawType::encapsulated::GUID_DEFINED => Some(FfsSection::Type::GuidDefined),
      FfsSectionRawType::encapsulated::DISPOSABLE => Some(FfsSection::Type::Disposable),
      FfsSectionRawType::PE32 => Some(FfsSection::Type::Pe32),
      FfsSectionRawType::PIC => Some(FfsSection::Type::Pic),
      FfsSectionRawType::TE => Some(FfsSection::Type::Te),
      FfsSectionRawType::DXE_DEPEX => Some(FfsSection::Type::DxeDepex),
      FfsSectionRawType::VERSION => Some(FfsSection::Type::Version),
      FfsSectionRawType::USER_INTERFACE => Some(FfsSection::Type::UserInterface),
      FfsSectionRawType::COMPATIBILITY16 => Some(FfsSection::Type::Compatibility16),
      FfsSectionRawType::FIRMWARE_VOLUME_IMAGE => Some(FfsSection::Type::FirmwareVolumeImage),
      FfsSectionRawType::FREEFORM_SUBTYPE_GUID => Some(FfsSection::Type::FreeformSubtypeGuid),
      FfsSectionRawType::RAW => Some(FfsSection::Type::Raw),
      FfsSectionRawType::PEI_DEPEX => Some(FfsSection::Type::PeiDepex),
      FfsSectionRawType::MM_DEPEX => Some(FfsSection::Type::MmDepex),
      _ => None,
    }
  }

  /// Returns the total section size (including the header and metadata, if any).
  pub fn section_size(&self) -> usize {
    self.header.section_size()
  }

  /// Returns the section data.
  pub fn section_data(&self) -> &[u8] {
    self.data
  }

  /// Returns the section metadata.
  pub fn metadata(&self) -> SectionMetaData {
    self.meta_data
  }

  /// Indicates whether this section is an encapsulation section.
  ///
  /// See PI spec 1.8A Section 2.1.5 for definition of encapsulation section vs. leaf section.
  pub fn is_encapsulation(&self) -> bool {
    self.section_type() == Some(FfsSection::Type::Compression)
      || self.section_type() == Some(FfsSection::Type::GuidDefined)
  }

  /// Returns the containing FFS file for the given section.
  pub fn containing_file(&self) -> File {
    self.containing_ffs
  }

  /// Returns the next section of the containing file.
  pub fn next_section(&self) -> Option<Section> {
    let mut next_section_address = self.base_address();
    next_section_address += self.section_size() as efi::PhysicalAddress;

    // per the PI spec, "The section headers aligned on 4 byte boundaries relative to the start of the file's image"
    // but, in fact, that just means "4-byte aligned" per the EDK2 implementation.
    next_section_address = align_up(next_section_address, 0x4);

    // check to see if we ran off the end of the file or containing extraction buffer yet.
    let top_address = match self.containing_extraction_buffer {
      Some(buffer) => buffer.as_ptr() as efi::PhysicalAddress + buffer.len() as efi::PhysicalAddress,
      None => self.containing_file().top_address(),
    };

    if next_section_address
      <= (top_address - mem::size_of::<FfsSectionHeader::CommonSectionHeaderStandard>() as efi::PhysicalAddress)
    {
      let mut next_section = unsafe { Section::new(self.containing_ffs, next_section_address).ok()? };
      next_section.containing_extraction_buffer = self.containing_extraction_buffer;
      return Some(next_section);
    }
    None
  }
}

impl fmt::Debug for Section {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Section @{:#x} type: {:x?} size: {:#x}", self.base_address(), self.section_type(), self.section_size())
  }
}

struct NullExtractor {}
impl SectionExtractor for NullExtractor {
  fn extract(&self, _section: Section) -> Vec<Section> {
    Vec::new()
  }
}

/// Iterator over sections within a file.
pub struct FfsSectionIterator {
  next_section: Option<Section>,
  extractor: Box<dyn SectionExtractor>,
  pending_encapsulated_sections: VecDeque<Section>,
}

impl FfsSectionIterator {
  /// Create a new section iterator with a no-op section extractor.
  /// Can be used for FVs that contain files with only leaf sections; any encapsulation sections will not be unpacked.
  pub fn new(start_section: Option<Section>) -> FfsSectionIterator {
    FfsSectionIterator {
      next_section: start_section,
      extractor: Box::new(NullExtractor {}),
      pending_encapsulated_sections: VecDeque::new(),
    }
  }

  /// Create a new section iterator with the specified extractor.
  /// When the iterator encounters an encapsulated section the given extractor will be used to extract the sections it
  /// contains and they will be added to the front of the iterator queue.
  pub fn new_with_extractor(
    start_section: Option<Section>,
    extractor: Box<dyn SectionExtractor>,
  ) -> FfsSectionIterator {
    FfsSectionIterator { next_section: start_section, extractor, pending_encapsulated_sections: VecDeque::new() }
  }
}

impl Iterator for FfsSectionIterator {
  type Item = Section;
  fn next(&mut self) -> Option<Section> {
    let current = {
      if self.pending_encapsulated_sections.is_empty() {
        let current = self.next_section?;
        self.next_section = current.next_section();
        Some(current)
      } else {
        self.pending_encapsulated_sections.pop_front()
      }
    };

    if let Some(section) = &current {
      if section.is_encapsulation() {
        let extracted_sections = self.extractor.extract(*section);
        for section in extracted_sections.into_iter().rev() {
          self.pending_encapsulated_sections.push_front(section);
        }
      }
    }
    current
  }
}
