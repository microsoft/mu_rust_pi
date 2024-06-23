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

use core::{fmt, mem};

use alloc::{collections::VecDeque, vec::Vec};
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
      FirmwareVolume,
    },
    fvb::attributes::raw::fvb2 as Fvb2RawAttributes,
  },
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum FfsFileHeader<'a> {
  Standard(&'a file::Header),
  Extended(&'a file::Header2),
}

impl<'a> FfsFileHeader<'a> {
  fn header(&self) -> &'a file::Header {
    match self {
      Self::Standard(header) => header,
      Self::Extended(header) => &header.header,
    }
  }

  fn size(&self) -> u64 {
    match self {
      Self::Standard(header) => {
        //add a byte to 24-bit size to get 32-bit size.
        let mut size_vec = header.size.to_vec();
        size_vec.push(0);
        u32::from_le_bytes(size_vec.try_into().unwrap()) as u64
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
}

impl<'a> TryFrom<&'a [u8]> for FfsFileHeader<'a> {
  type Error = efi::Status;
  fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
    // check that value has enough space for a standard header
    if mem::size_of::<file::Header>() > value.len() {
      Err(efi::Status::INVALID_PARAMETER)?;
    }

    // safety: confirmed there is enough space in value to hold the header.
    let file = value.as_ptr() as *const file::Header;
    unsafe {
      if (*file).attributes & LARGE_FILE != 0 {
        //extended header. check that there is enough space in the buffer for the extra fields.
        if mem::size_of::<file::Header2>() > value.len() {
          Err(efi::Status::INVALID_PARAMETER)?;
        }
        Ok(Self::Extended(&*(file as *const file::Header2)))
      } else {
        Ok(Self::Standard(&*file))
      }
    }
  }
}

/// Firmware File System (FFS) File.
#[derive(Copy, Clone)]
pub struct File<'a> {
  containing_fv: &'a FirmwareVolume<'a>,
  file_offset: usize,
  file_header: FfsFileHeader<'a>,
  file_data: &'a [u8],
}

impl<'a> File<'a> {
  /// Instantiate a new File structure given the containing volume and base address.
  ///
  /// ## Safety
  /// Caller must ensure that base_address points to the start of a valid FFS header and that it is safe to access
  /// memory from the start of that header to the full length fo the file specified by that header. Caller must also
  /// ensure that the memory containing the file data outlives this File instance.
  ///
  /// Various sanity checks will be performed by this routine to ensure File consistency.
  pub fn new(containing_fv: &'a FirmwareVolume, file_offset: usize) -> Result<File<'a>, efi::Status> {
    let fv_data = containing_fv.fv_data_buffer();

    //check that fv_data has enough space for a standard file header.
    if file_offset + mem::size_of::<file::Header>() > fv_data.len() {
      Err(efi::Status::INVALID_PARAMETER)?;
    }

    let file_header = fv_data[file_offset..].try_into()?;

    Ok(File {
      containing_fv,
      file_offset,
      file_header,
      file_data: &fv_data[file_offset..file_offset + file_header.size() as usize],
    })
  }

  /// Returns the file size (including header).
  pub fn file_size(&self) -> u64 {
    self.file_header.size()
  }

  /// Returns file data size (not including header).
  pub fn file_data_size(&self) -> u64 {
    self.file_header.size() - self.file_header.data_offset() as u64
  }

  /// Returns the file type.
  pub fn file_type(&self) -> Option<FfsFileType> {
    match self.file_header.header().file_type {
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
    let attributes = self.file_header.header().attributes;
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
    self.file_header.header().name
  }

  /// Returns the file contents (not including the header).
  pub fn file_data(&self) -> &'a [u8] {
    &self.file_data[self.file_header.data_offset()..]
  }

  /// Returns the next file in the Firmware Volume, if any.
  pub fn next_ffs_file(&self) -> Option<File<'a>> {
    // per the PI spec, "Given a file F, the next file FvHeader is located at the next 8-byte aligned firmware volume
    // offset following the last byte the file F"
    // but, in fact, that just means "8-byte aligned" per the EDK2 implementation.
    let next_file_offset = align_up(self.file_offset as u64 + self.file_size(), 0x8) as usize;

    //check to see if the offset is off the end of the FV
    if next_file_offset + mem::size_of::<file::Header>() > self.containing_fv.fv_data_buffer().len() {
      return None;
    }

    //check if the rest of the FV is full of erase bytes
    let erase_byte: u8 =
      if self.containing_fv.attributes() & Fvb2RawAttributes::ERASE_POLARITY != 0 { 0xff } else { 0 };
    if self.containing_fv.fv_data_buffer()[next_file_offset..].iter().all(|&x| x == erase_byte) {
      return None;
    }

    // Sanity checking of file header is done in constructor.
    File::new(self.containing_fv, next_file_offset).ok()
  }

  /// Returns the first section of the file, if any.
  pub fn first_ffs_section(&'a self) -> Option<Section<'a>> {
    // handle the scenario where there isn't enough room for even a single section.
    if self.file_size() <= (self.file_header.data_offset() + mem::size_of::<CommonSectionHeaderStandard>()) as u64 {
      return None;
    }

    //Sanity checking of the section header is done in constructor.
    Section::new(self, self.file_header.data_offset(), self.file_data).ok()
  }

  /// Returns an iterator over the sections of the file.
  pub fn ffs_sections(&self) -> impl Iterator<Item = Section> {
    FfsSectionIterator::new(self.first_ffs_section())
  }

  /// Returns an iterator over the sections of the file, using the provided section extractor.
  pub fn ffs_sections_with_extractor(
    &'a self,
    extractor: &'a dyn SectionExtractor,
  ) -> impl Iterator<Item = Section> + 'a {
    FfsSectionIterator::new_with_extractor(self.first_ffs_section(), extractor)
  }

  /// Returns the raw file type.
  pub fn file_type_raw(&self) -> u8 {
    self.file_header.header().file_type
  }

  /// Returns the raw file attributes.
  pub fn file_attributes_raw(&self) -> u8 {
    self.file_header.header().attributes
  }

  /// Returns the base address of the containing FV.
  pub fn containing_fv_data(&self) -> &'a [u8] {
    self.containing_fv.fv_data_buffer()
  }
}

impl<'a> fmt::Debug for File<'a> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "File @{:#x?} type: {:?} name: {:?} size: {:?}",
      self.containing_fv_data().as_ptr(),
      self.file_type(),
      Uuid::from_bytes_le(*self.file_name().as_bytes()),
      self.file_size()
    )
  }
}

pub(crate) struct FileIterator<'a> {
  next_ffs: Option<File<'a>>,
}

impl<'a> FileIterator<'a> {
  pub fn new(start_file: Option<File<'a>>) -> FileIterator<'a> {
    FileIterator { next_ffs: start_file }
  }
}

impl<'a> Iterator for FileIterator<'a> {
  type Item = File<'a>;
  fn next(&mut self) -> Option<File<'a>> {
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
enum CommonSectionHeader<'a> {
  Standard(&'a FfsSectionHeader::CommonSectionHeaderStandard),
  Extended(&'a FfsSectionHeader::CommonSectionHeaderExtended),
}

impl<'a> CommonSectionHeader<'a> {
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

  fn header_size(&self) -> usize {
    match self {
      CommonSectionHeader::Standard(_) => mem::size_of::<CommonSectionHeaderStandard>(),
      CommonSectionHeader::Extended(_) => mem::size_of::<CommonSectionHeaderExtended>(),
    }
  }
}

impl<'a> TryFrom<&'a [u8]> for CommonSectionHeader<'a> {
  type Error = efi::Status;
  fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
    if mem::size_of::<CommonSectionHeaderStandard>() > value.len() {
      Err(efi::Status::INVALID_PARAMETER)?;
    }

    //safety: check above guarantees that buffer can hold at least standard header size.
    let header = unsafe { &*(value.as_ptr() as *const CommonSectionHeaderStandard) };

    if header.size.iter().all(|&x| x == 0xff) {
      //check if extended header can fit in the buffer.
      if mem::size_of::<CommonSectionHeaderExtended>() > value.len() {
        Err(efi::Status::INVALID_PARAMETER)?;
      }
      Ok(CommonSectionHeader::Extended(unsafe { &*(value.as_ptr() as *const CommonSectionHeaderExtended) }))
    } else {
      Ok(CommonSectionHeader::Standard(header))
    }
  }
}

/// Section metadata
#[derive(Debug, Clone, Copy)]
pub enum SectionMetaData<'a> {
  None,
  Compression(&'a FfsSectionHeader::Compression),
  GuidDefined(&'a FfsSectionHeader::GuidDefined),
  Version(&'a FfsSectionHeader::Version),
  FreeformSubtypeGuid(&'a FfsSectionHeader::FreeformSubtypeGuid),
}

/// Firmware File System (FFS) Section.
#[derive(Clone, Copy)]
pub struct Section<'a> {
  containing_ffs: &'a File<'a>,
  section_offset: usize,
  section_header: CommonSectionHeader<'a>,
  meta_data: SectionMetaData<'a>,
  containing_buffer: &'a [u8],
  section_data: &'a [u8],
}

impl<'a> Section<'a> {
  pub fn new(
    containing_ffs: &'a File<'a>,
    section_offset: usize,
    containing_buffer: &'a [u8],
  ) -> Result<Section<'a>, efi::Status> {
    //check that section_buffer has enough space for a standard section header.
    if section_offset + mem::size_of::<CommonSectionHeaderStandard>() > containing_buffer.len() {
      Err(efi::Status::INVALID_PARAMETER)?;
    }

    let section_header: CommonSectionHeader = containing_buffer[section_offset..].try_into()?;

    //offset of meta data (if present) or actual data
    let content_offset = section_offset + section_header.header_size();
    let section_end = section_offset + section_header.section_size();

    let (meta_data, section_data) = match section_header.section_type() {
      FfsSectionRawType::encapsulated::COMPRESSION => {
        //check if compression header can fit in the section buffer
        let compression_header_size = mem::size_of::<FfsSectionHeader::Compression>();
        if content_offset + compression_header_size > containing_buffer.len() {
          Err(efi::Status::INVALID_PARAMETER)?
        }

        //safety: above checks confirm that compression metadata fits in the section buffer, so safe to cast to a ref.
        let meta_data = unsafe {
          SectionMetaData::Compression(
            &*(containing_buffer[content_offset..].as_ptr() as *const FfsSectionHeader::Compression),
          )
        };

        let data_offset = content_offset + compression_header_size;
        (meta_data, &containing_buffer[data_offset..section_end])
      }
      FfsSectionRawType::encapsulated::GUID_DEFINED => {
        //check if guid-defined header can fit in the section buffer
        let guid_defined_header_size = mem::size_of::<FfsSectionHeader::GuidDefined>();
        if content_offset + guid_defined_header_size > containing_buffer.len() {
          Err(efi::Status::INVALID_PARAMETER)?
        }

        //safety: above checks confirm that guid_defined metadata fits in the section buffer, so safe to cast to a ref.
        let guid_defined_header =
          unsafe { &*(containing_buffer[content_offset..].as_ptr() as *const FfsSectionHeader::GuidDefined) };

        let data_offset = guid_defined_header.data_offset as usize;
        let meta_data = SectionMetaData::GuidDefined(guid_defined_header);
        (meta_data, &containing_buffer[data_offset..section_end])
      }
      FfsSectionRawType::VERSION => {
        //check if version header can fit in the section buffer
        let version_header_size = mem::size_of::<FfsSectionHeader::Version>();
        if content_offset + version_header_size > containing_buffer.len() {
          Err(efi::Status::INVALID_PARAMETER)?
        }

        //safety: above checks confirm that version metadata fits in the section buffer, so safe to cast to a ref.
        let meta_data = unsafe {
          SectionMetaData::Version(&*(containing_buffer[content_offset..].as_ptr() as *const FfsSectionHeader::Version))
        };

        let data_offset = content_offset + version_header_size;
        (meta_data, &containing_buffer[data_offset..section_end])
      }
      FfsSectionRawType::FREEFORM_SUBTYPE_GUID => {
        //check if freeform_guid header can fit in the section buffer
        let free_form_subtype_header = mem::size_of::<FfsSectionHeader::FreeformSubtypeGuid>();
        if content_offset + free_form_subtype_header > containing_buffer.len() {
          Err(efi::Status::INVALID_PARAMETER)?
        }

        //safety: above checks confirm that freeform_guid metadata fits in the section buffer, so safe to cast to a ref.
        let meta_data = unsafe {
          SectionMetaData::FreeformSubtypeGuid(
            &*(containing_buffer[content_offset..].as_ptr() as *const FfsSectionHeader::FreeformSubtypeGuid),
          )
        };

        let data_offset = content_offset + free_form_subtype_header;
        (meta_data, &containing_buffer[data_offset..section_end])
      }
      _ => (SectionMetaData::None, &containing_buffer[content_offset..section_end]),
    };

    Ok(Section { containing_ffs, section_offset, section_header, meta_data, containing_buffer, section_data })
  }

  /// Returns the section type.
  pub fn section_type(&self) -> Option<FfsSection::Type> {
    match self.section_header.section_type() {
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
    self.section_header.section_size()
  }

  /// Returns the section data.
  pub fn section_data(&self) -> &[u8] {
    self.section_data
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
  pub fn containing_file(&self) -> &'a File<'a> {
    self.containing_ffs
  }

  /// Returns the next section of the containing file.
  pub fn next_section(&self) -> Option<Section<'a>> {
    // per the PI spec, "The section headers aligned on 4 byte boundaries relative to the start of the file's image"
    // but, in fact, that just means "4-byte aligned" per the EDK2 implementation.
    let next_section_offset = align_up((self.section_offset + self.section_size()) as u64, 0x4) as usize;

    // check to see if the offset is off the end of the containing buffer.
    if next_section_offset + mem::size_of::<CommonSectionHeaderStandard>() > self.containing_buffer.len() {
      return None;
    }

    Section::new(self.containing_ffs, next_section_offset, self.containing_buffer).ok()
  }
}

impl<'a> fmt::Debug for Section<'a> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "Section @{:#x?} type: {:x?} size: {:#x}",
      self.section_data().as_ptr(),
      self.section_type(),
      self.section_size()
    )
  }
}

/// Iterator over sections within a file.
pub struct FfsSectionIterator<'a> {
  next_section: Option<Section<'a>>,
  extractor: Option<&'a dyn SectionExtractor>,
  pending_encapsulated_sections: VecDeque<Section<'a>>,
}

impl<'a> FfsSectionIterator<'a> {
  /// Create a new section iterator with a no-op section extractor.
  /// Can be used for FVs that contain files with only leaf sections; any encapsulation sections will not be unpacked.
  pub fn new(start_section: Option<Section<'a>>) -> FfsSectionIterator<'a> {
    FfsSectionIterator { next_section: start_section, extractor: None, pending_encapsulated_sections: VecDeque::new() }
  }

  /// Create a new section iterator with the specified extractor.
  /// When the iterator encounters an encapsulated section the given extractor will be used to extract the sections it
  /// contains and they will be added to the front of the iterator queue.
  pub fn new_with_extractor(
    start_section: Option<Section<'a>>,
    extractor: &'a dyn SectionExtractor,
  ) -> FfsSectionIterator<'a> {
    FfsSectionIterator {
      next_section: start_section,
      extractor: Some(extractor),
      pending_encapsulated_sections: VecDeque::new(),
    }
  }
}

impl<'a> Iterator for FfsSectionIterator<'a> {
  type Item = Section<'a>;
  fn next(&mut self) -> Option<Section<'a>> {
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
        if let Some(extractor) = self.extractor {
          let extracted_sections = extractor.extract(*section);
          for section in extracted_sections.into_iter().rev() {
            self.pending_encapsulated_sections.push_front(section);
          }
        }
      }
    }
    current
  }
}
