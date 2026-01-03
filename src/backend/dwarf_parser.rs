//! DWARF Debug Info Parser using TypeTable
//!
//! This module parses DWARF debug information from ELF files and populates
//! a TypeTable with type definitions. It uses a simpler approach than the
//! original elf_parser.rs by leveraging the TypeTable's TypeId-based references.
//!
//! The parsing is done in a single pass with deferred resolution:
//! 1. Allocate TypeIds for all DIEs as we encounter them
//! 2. Parse type definitions, using TypeIds for references
//! 3. After all types are parsed, resolve forward declarations
//!
//! Template support is included by parsing DW_TAG_template_type_parameter
//! and DW_TAG_template_value_parameter DIEs.

use super::type_table::{
    BaseClassDef, DwarfTypeKey, EnumDef, EnumVariant, ForwardDeclKind, GlobalTypeKey, MemberDef,
    PrimitiveDef, StructDef, TemplateParam, TypeDef, TypeId, TypeTable,
};
use gimli::{
    AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, ReaderOffset, RunTimeEndian,
    Unit, UnitOffset,
};
use object::{Object, ObjectSection};
use std::borrow::Cow;

/// A parsed symbol with type information
#[derive(Debug, Clone)]
pub struct ParsedSymbol {
    pub name: String,
    pub mangled_name: Option<String>,
    pub address: u64,
    pub size: u64,
    pub type_id: TypeId,
    pub is_global: bool,
}

/// Result of parsing DWARF info
#[derive(Debug)]
pub struct DwarfParseResult {
    pub type_table: TypeTable,
    pub symbols: Vec<ParsedSymbol>,
    /// Mapping from variable name (including mangled names) to TypeId
    /// This includes variables without addresses that couldn't become full symbols
    pub name_to_type: std::collections::HashMap<String, TypeId>,
}

/// DWARF parser that populates a TypeTable
pub struct DwarfParser<'a, R: gimli::Reader> {
    dwarf: &'a Dwarf<R>,
    type_table: TypeTable,
    /// Maps from variable DIE key to its parsed info for later type resolution
    pending_symbols: Vec<(DwarfTypeKey, PendingSymbol)>,
}

/// A symbol that's pending type resolution
#[derive(Debug)]
struct PendingSymbol {
    name: String,
    mangled_name: Option<String>,
    address: Option<u64>,
    size: u64,
    type_key: Option<DwarfTypeKey>,
    is_global: bool,
}

type Reader<'a> = EndianSlice<'a, RunTimeEndian>;

impl<'a> DwarfParser<'a, Reader<'a>> {
    /// Parse DWARF info from raw bytes
    pub fn parse_bytes(data: &'a [u8]) -> Result<DwarfParseResult, String> {
        let file =
            object::File::parse(data).map_err(|e| format!("Failed to parse object: {}", e))?;

        let endian = if file.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        };

        // Load DWARF sections
        let load_section = |id: gimli::SectionId| -> Result<Cow<[u8]>, gimli::Error> {
            Ok(file
                .section_by_name(id.name())
                .and_then(|s| s.data().ok())
                .map(Cow::Borrowed)
                .unwrap_or(Cow::Borrowed(&[])))
        };

        let dwarf_cow =
            gimli::Dwarf::load(load_section).map_err(|e| format!("Failed to load DWARF: {}", e))?;

        let dwarf = dwarf_cow.borrow(|section| EndianSlice::new(section, endian));

        let mut parser = DwarfParser {
            dwarf: &dwarf,
            type_table: TypeTable::new(),
            pending_symbols: Vec::new(),
        };

        parser.parse_all_units()?;

        Ok(parser.finish())
    }

    /// Parse all compilation units
    fn parse_all_units(&mut self) -> Result<(), String> {
        let mut units = self.dwarf.units();
        let mut unit_index = 0usize;

        while let Some(header) = units
            .next()
            .map_err(|e| format!("Failed to read DWARF unit: {}", e))?
        {
            let unit = self
                .dwarf
                .unit(header)
                .map_err(|e| format!("Failed to parse DWARF unit: {}", e))?;

            self.parse_unit(&unit, unit_index)?;
            unit_index += 1;
        }

        Ok(())
    }

    /// Parse a single compilation unit
    fn parse_unit(&mut self, unit: &Unit<Reader<'a>>, unit_index: usize) -> Result<(), String> {
        let mut entries = unit.entries();

        while let Some((_, entry)) = entries
            .next_dfs()
            .map_err(|e| format!("Failed to read DWARF entry: {}", e))?
        {
            self.parse_entry(unit, unit_index, entry)?;
        }

        Ok(())
    }

    /// Parse a single DIE
    fn parse_entry(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Result<(), String> {
        let offset: usize = entry.offset().0.into_u64() as usize;
        let key = DwarfTypeKey::new(unit_index, offset);

        // Also compute the global offset for this DIE so we can register it as an alias.
        // ARM compilers often use DebugInfoRef (global offsets) instead of UnitRef (local offsets).
        let global_offset = entry
            .offset()
            .to_debug_info_offset(&unit.header)
            .map(|o| o.0.into_u64() as usize);

        match entry.tag() {
            // Type DIEs
            gimli::DW_TAG_base_type => {
                self.parse_base_type(unit, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_pointer_type => {
                self.parse_pointer_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_reference_type | gimli::DW_TAG_rvalue_reference_type => {
                self.parse_reference_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_const_type => {
                self.parse_const_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_volatile_type => {
                self.parse_volatile_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_restrict_type => {
                self.parse_restrict_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_typedef => {
                self.parse_typedef(unit, unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_array_type => {
                self.parse_array_type(unit, unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_structure_type | gimli::DW_TAG_class_type => {
                self.parse_struct_type(unit, unit_index, key, entry, false);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_union_type => {
                self.parse_struct_type(unit, unit_index, key, entry, true);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_enumeration_type => {
                self.parse_enum_type(unit, unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_subroutine_type => {
                self.parse_subroutine_type(unit_index, key, entry);
                self.register_global_alias(key, global_offset);
            }
            gimli::DW_TAG_unspecified_type => {
                // Usually represents void
                self.type_table.insert_for_key(key, TypeDef::Void);
                self.register_global_alias(key, global_offset);
            }

            // Variable/symbol DIEs
            gimli::DW_TAG_variable => {
                self.parse_variable(unit, unit_index, key, entry);
            }

            _ => {
                // Ignore other tags
            }
        }

        Ok(())
    }

    /// Parse DW_TAG_base_type
    fn parse_base_type(
        &mut self,
        unit: &Unit<Reader<'a>>,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let name = self.get_name(unit, entry).unwrap_or_default();
        let size = self.get_byte_size(entry).unwrap_or(0);
        let encoding = self.get_encoding(entry);

        let prim = match encoding {
            Some(gimli::DW_ATE_boolean) => PrimitiveDef::Bool,
            Some(gimli::DW_ATE_signed_char) => PrimitiveDef::SignedChar,
            Some(gimli::DW_ATE_unsigned_char) => PrimitiveDef::UnsignedChar,
            Some(gimli::DW_ATE_signed) => match size {
                1 => PrimitiveDef::SignedChar,
                2 => PrimitiveDef::Short,
                4 => PrimitiveDef::Int,
                8 => PrimitiveDef::LongLong,
                _ => PrimitiveDef::SizedInt { size, signed: true },
            },
            Some(gimli::DW_ATE_unsigned) => match size {
                1 => PrimitiveDef::UnsignedChar,
                2 => PrimitiveDef::UnsignedShort,
                4 => PrimitiveDef::UnsignedInt,
                8 => PrimitiveDef::UnsignedLongLong,
                _ => PrimitiveDef::SizedInt {
                    size,
                    signed: false,
                },
            },
            Some(gimli::DW_ATE_float) => match size {
                4 => PrimitiveDef::Float,
                8 => PrimitiveDef::Double,
                16 => PrimitiveDef::LongDouble,
                _ => PrimitiveDef::SizedFloat { size },
            },
            Some(gimli::DW_ATE_UTF) => PrimitiveDef::Char, // Unicode char
            _ => {
                // Try to infer from name
                match name.as_str() {
                    "bool" | "_Bool" => PrimitiveDef::Bool,
                    "char" => PrimitiveDef::Char,
                    "signed char" => PrimitiveDef::SignedChar,
                    "unsigned char" => PrimitiveDef::UnsignedChar,
                    "short" | "short int" => PrimitiveDef::Short,
                    "unsigned short" | "short unsigned int" => PrimitiveDef::UnsignedShort,
                    "int" => PrimitiveDef::Int,
                    "unsigned int" | "unsigned" => PrimitiveDef::UnsignedInt,
                    "long" | "long int" => PrimitiveDef::Long,
                    "unsigned long" | "long unsigned int" => PrimitiveDef::UnsignedLong,
                    "long long" | "long long int" => PrimitiveDef::LongLong,
                    "unsigned long long" | "long long unsigned int" => {
                        PrimitiveDef::UnsignedLongLong
                    }
                    "float" => PrimitiveDef::Float,
                    "double" => PrimitiveDef::Double,
                    "long double" => PrimitiveDef::LongDouble,
                    _ => PrimitiveDef::SizedInt { size, signed: true },
                }
            }
        };

        self.type_table
            .insert_for_key(key, TypeDef::Primitive(prim));
    }

    /// Parse DW_TAG_pointer_type
    fn parse_pointer_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let inner_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            // Void pointer
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Pointer(inner_id));
    }

    /// Parse DW_TAG_reference_type
    fn parse_reference_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let inner_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Reference(inner_id));
    }

    /// Parse DW_TAG_const_type
    fn parse_const_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let inner_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Const(inner_id));
    }

    /// Parse DW_TAG_volatile_type
    fn parse_volatile_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let inner_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Volatile(inner_id));
    }

    /// Parse DW_TAG_restrict_type
    fn parse_restrict_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let inner_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Restrict(inner_id));
    }

    /// Parse DW_TAG_typedef
    fn parse_typedef(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let name = self.get_name(unit, entry).unwrap_or_default();

        let underlying = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        self.type_table
            .insert_for_key(key, TypeDef::Typedef { name, underlying });
    }

    /// Parse DW_TAG_array_type
    fn parse_array_type(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let element_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            self.ensure_void_type()
        };

        // Get array count from subrange child
        let count = self.get_array_count(unit, entry);

        self.type_table.insert_for_key(
            key,
            TypeDef::Array {
                element: element_id,
                count,
            },
        );
    }

    /// Parse DW_TAG_structure_type, DW_TAG_class_type, or DW_TAG_union_type
    fn parse_struct_type(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
        is_union: bool,
    ) {
        let name = self.get_name(unit, entry);
        let size = self.get_byte_size(entry).unwrap_or(0);
        let is_class = entry.tag() == gimli::DW_TAG_class_type;
        let is_declaration = self.has_declaration_attr(entry);

        // Check if this is a forward declaration
        if is_declaration {
            let kind = if is_union {
                ForwardDeclKind::Union
            } else if is_class {
                ForwardDeclKind::Class
            } else {
                ForwardDeclKind::Struct
            };

            self.type_table.insert_for_key(
                key,
                TypeDef::ForwardDecl {
                    name: name.unwrap_or_default(),
                    kind,
                    target: None,
                },
            );
            return;
        }

        // Pre-allocate the TypeId so members can reference it (for recursive types)
        let struct_id = self.type_table.get_or_allocate(key);

        let mut struct_def = StructDef::new(name, size, is_class);

        // Parse children (members, base classes, template params)
        if let Ok(mut tree) = unit.entries_tree(Some(entry.offset())) {
            if let Ok(root) = tree.root() {
                let mut children = root.children();
                while let Ok(Some(child)) = children.next() {
                    let child_entry = child.entry();
                    match child_entry.tag() {
                        gimli::DW_TAG_member => {
                            if let Some(member) = self.parse_member(unit, unit_index, child_entry) {
                                struct_def.members.push(member);
                            }
                        }
                        gimli::DW_TAG_inheritance => {
                            if let Some(base) =
                                self.parse_inheritance(unit, unit_index, child_entry)
                            {
                                struct_def.base_classes.push(base);
                            }
                        }
                        gimli::DW_TAG_template_type_parameter => {
                            if let Some(param) =
                                self.parse_template_type_param(unit, unit_index, child_entry)
                            {
                                struct_def.template_params.push(param);
                            }
                        }
                        gimli::DW_TAG_template_value_parameter => {
                            if let Some(param) = self.parse_template_value_param(unit, child_entry)
                            {
                                struct_def.template_params.push(param);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let def = if is_union {
            TypeDef::Union(struct_def)
        } else {
            TypeDef::Struct(struct_def)
        };

        self.type_table.define(struct_id, def);
    }

    /// Parse DW_TAG_member
    fn parse_member(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<MemberDef> {
        let name = self.get_name(unit, entry).unwrap_or_default();
        let offset = self.get_member_offset(unit, entry).unwrap_or(0);

        let type_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            TypeId::INVALID
        };

        let mut member = MemberDef::new(name, offset, type_id);
        member.bit_offset = self.get_bit_offset(entry);
        member.bit_size = self.get_bit_size(entry);

        Some(member)
    }

    /// Parse DW_TAG_inheritance
    fn parse_inheritance(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<BaseClassDef> {
        let base_type_id = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            self.type_table.get_or_allocate(ref_key.to_dwarf_key())
        } else {
            return None;
        };

        let offset = self.get_member_offset(unit, entry).unwrap_or(0);
        let is_virtual = self.get_virtuality(entry).is_some();

        Some(BaseClassDef {
            type_id: base_type_id,
            offset,
            is_virtual,
        })
    }

    /// Parse DW_TAG_template_type_parameter
    fn parse_template_type_param(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<TemplateParam> {
        let name = self
            .get_name(unit, entry)
            .unwrap_or_else(|| "T".to_string());

        let type_id = if let Some(type_offset) = self.get_type_ref(entry) {
            let ref_key = DwarfTypeKey::new(unit_index, type_offset.0.into_u64() as usize);
            self.type_table.get_or_allocate(ref_key)
        } else {
            self.ensure_void_type()
        };

        Some(TemplateParam::Type { name, type_id })
    }

    /// Parse DW_TAG_template_value_parameter
    fn parse_template_value_param(
        &mut self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<TemplateParam> {
        let name = self
            .get_name(unit, entry)
            .unwrap_or_else(|| "N".to_string());
        let value = self.get_const_value(entry).unwrap_or(0);

        Some(TemplateParam::Value { name, value })
    }

    /// Parse DW_TAG_enumeration_type
    fn parse_enum_type(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let name = self.get_name(unit, entry);
        let size = self.get_byte_size(entry).unwrap_or(4);
        let is_declaration = self.has_declaration_attr(entry);

        // Check if this is a forward declaration
        if is_declaration {
            self.type_table.insert_for_key(
                key,
                TypeDef::ForwardDecl {
                    name: name.unwrap_or_default(),
                    kind: ForwardDeclKind::Enum,
                    target: None,
                },
            );
            return;
        }

        // Check for C++11 enum class (DW_AT_enum_class)
        let is_scoped = self.has_enum_class_attr(entry);

        // Get underlying type if specified
        let underlying_type = if let Some(type_offset) = self.get_type_ref(entry) {
            let ref_key = DwarfTypeKey::new(unit_index, type_offset.0.into_u64() as usize);
            Some(self.type_table.get_or_allocate(ref_key))
        } else {
            None
        };

        let mut enum_def = EnumDef::new(name, size, is_scoped);
        enum_def.underlying_type = underlying_type;

        // Parse enumerator children
        if let Ok(mut tree) = unit.entries_tree(Some(entry.offset())) {
            if let Ok(root) = tree.root() {
                let mut children = root.children();
                while let Ok(Some(child)) = children.next() {
                    if child.entry().tag() == gimli::DW_TAG_enumerator {
                        if let Some(variant) = self.parse_enumerator(unit, child.entry()) {
                            enum_def.variants.push(variant);
                        }
                    }
                }
            }
        }

        self.type_table.insert_for_key(key, TypeDef::Enum(enum_def));
    }

    /// Parse DW_TAG_enumerator
    fn parse_enumerator(
        &mut self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<EnumVariant> {
        let name = self.get_name(unit, entry)?;
        let value = self.get_const_value(entry).unwrap_or(0);
        Some(EnumVariant { name, value })
    }

    /// Parse DW_TAG_subroutine_type
    fn parse_subroutine_type(
        &mut self,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        let return_type = if let Some(ref_key) = self.get_type_ref_key(unit_index, entry) {
            Some(self.type_table.get_or_allocate(ref_key.to_dwarf_key()))
        } else {
            None // void return
        };

        // Note: We could parse parameter types from children, but for now keep it simple
        self.type_table.insert_for_key(
            key,
            TypeDef::Subroutine {
                return_type,
                params: Vec::new(),
            },
        );
    }

    /// Parse DW_TAG_variable
    fn parse_variable(
        &mut self,
        unit: &Unit<Reader<'a>>,
        unit_index: usize,
        key: DwarfTypeKey,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) {
        // Get name (try linkage name first, then regular name)
        let linkage_name = self.get_linkage_name(unit, entry);
        let display_name = self.get_name(unit, entry);

        let name = linkage_name.clone().or_else(|| display_name.clone());

        let name = match name {
            Some(n) if !n.is_empty() => n,
            _ => return, // Skip anonymous variables
        };

        // Get address if available (variables without addresses are still useful for type mapping)
        let address = self.get_variable_location(unit, entry);

        let type_key = self
            .get_type_ref_key(unit_index, entry)
            .map(|k| k.to_dwarf_key());

        // Skip variables without type information
        if type_key.is_none() {
            return;
        }

        let is_external = self.has_external_attr(entry);

        let pending = PendingSymbol {
            name: display_name.unwrap_or_else(|| name.clone()),
            mangled_name: if linkage_name.as_ref() != Some(&name) {
                linkage_name
            } else {
                None
            },
            address,
            size: self.get_byte_size(entry).unwrap_or(0),
            type_key,
            is_global: is_external,
        };

        self.pending_symbols.push((key, pending));
    }

    /// Finish parsing and return the result
    fn finish(mut self) -> DwarfParseResult {
        // Resolve forward declarations
        self.type_table.resolve_forward_declarations();

        // Resolve inherited members
        self.resolve_inherited_members();

        // Build name-to-type mapping for ALL variables (including those without addresses)
        // and separate out symbols with addresses
        let mut name_to_type = std::collections::HashMap::new();
        let mut symbols = Vec::new();

        for (_, pending) in self.pending_symbols {
            let type_id = pending
                .type_key
                .and_then(|k| self.type_table.get_by_dwarf_key(k))
                .unwrap_or(TypeId::INVALID);

            if !type_id.is_valid() {
                continue;
            }

            // Add to name-to-type mapping (for all variables)
            name_to_type.insert(pending.name.clone(), type_id);
            if let Some(ref mangled) = pending.mangled_name {
                name_to_type.insert(mangled.clone(), type_id);
            }

            // Only create full symbols for variables with addresses
            if let Some(address) = pending.address {
                // Get size from type if not specified
                let size = if pending.size > 0 {
                    pending.size
                } else {
                    self.type_table.type_size(type_id).unwrap_or(0)
                };

                symbols.push(ParsedSymbol {
                    name: pending.name,
                    mangled_name: pending.mangled_name,
                    address,
                    size,
                    type_id,
                    is_global: pending.is_global,
                });
            }
        }

        let stats = self.type_table.stats();

        // Count how many pending symbols had valid vs invalid type resolution
        let _total_pending = symbols.len() + name_to_type.len();
        let with_address = symbols.len();

        tracing::debug!(
            "DWARF parsing complete: {} types, {} structs, {} forward decls ({} unresolved)",
            stats.total_types,
            stats.structs,
            stats.forward_decls,
            stats.unresolved_forward_decls,
        );

        tracing::debug!(
            "DWARF variables: {} with addresses, {} without (name-to-type mappings for fallback matching)",
            with_address,
            name_to_type.len().saturating_sub(with_address)
        );

        // Log info about global vs local key usage for debugging ARM/AXF issues
        let dwarf_key_count = self.type_table.dwarf_key_count();
        tracing::debug!(
            "DWARF key mappings: {} total (includes global offset aliases for DebugInfoRef support)",
            dwarf_key_count
        );

        DwarfParseResult {
            type_table: self.type_table,
            symbols,
            name_to_type,
        }
    }

    /// Resolve inherited members from base classes
    fn resolve_inherited_members(&mut self) {
        // Collect all struct/union type IDs
        let struct_ids: Vec<TypeId> = self
            .type_table
            .all_type_ids()
            .filter(|&id| {
                matches!(
                    self.type_table.get(id),
                    Some(TypeDef::Struct(_)) | Some(TypeDef::Union(_))
                )
            })
            .collect();

        for id in struct_ids {
            self.resolve_inherited_members_for(id, &mut std::collections::HashSet::new());
        }
    }

    fn resolve_inherited_members_for(
        &mut self,
        id: TypeId,
        visited: &mut std::collections::HashSet<TypeId>,
    ) {
        if visited.contains(&id) {
            return; // Prevent infinite recursion
        }
        visited.insert(id);

        // Get base classes for this struct
        let base_classes: Vec<BaseClassDef> = match self.type_table.get(id) {
            Some(TypeDef::Struct(s)) | Some(TypeDef::Union(s)) => s.base_classes.clone(),
            _ => return,
        };

        if base_classes.is_empty() {
            return;
        }

        // Collect members from base classes
        let mut inherited_members = Vec::new();
        for base in &base_classes {
            // First resolve the base class's inherited members
            let resolved_base_id = self.type_table.resolve(base.type_id);
            self.resolve_inherited_members_for(resolved_base_id, visited);

            // Then collect its members
            if let Some(members) = self.type_table.get_members(resolved_base_id) {
                for member in members {
                    inherited_members.push(MemberDef {
                        name: member.name.clone(),
                        offset: member.offset + base.offset,
                        type_id: member.type_id,
                        bit_offset: member.bit_offset,
                        bit_size: member.bit_size,
                    });
                }
            }
        }

        // Add inherited members to the struct
        if !inherited_members.is_empty() {
            if let Some(TypeDef::Struct(s)) | Some(TypeDef::Union(s)) = self.type_table.get_mut(id)
            {
                // Insert inherited members at the beginning
                inherited_members.append(&mut s.members);
                s.members = inherited_members;
            }
        }
    }

    /// Ensure we have a void type and return its ID
    fn ensure_void_type(&mut self) -> TypeId {
        // Look for an existing void type
        for (id, def) in self.type_table.iter() {
            if matches!(def, TypeDef::Void) {
                return id;
            }
        }
        // Create one if not found
        self.type_table.insert(TypeDef::Void)
    }

    // ==================== Attribute Helpers ====================

    fn get_name(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<String> {
        let attr = entry.attr_value(gimli::DW_AT_name).ok()??;
        self.attr_to_string(unit, &attr)
    }

    fn get_linkage_name(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<String> {
        // Try DW_AT_linkage_name first, then DW_AT_MIPS_linkage_name
        let attr = entry
            .attr_value(gimli::DW_AT_linkage_name)
            .ok()?
            .or_else(|| entry.attr_value(gimli::DW_AT_MIPS_linkage_name).ok()?);
        attr.and_then(|a| self.attr_to_string(unit, &a))
    }

    fn attr_to_string(
        &self,
        unit: &Unit<Reader<'a>>,
        attr: &AttributeValue<Reader<'a>>,
    ) -> Option<String> {
        match attr {
            AttributeValue::DebugStrRef(offset) => self
                .dwarf
                .debug_str
                .get_str(*offset)
                .ok()
                .map(|s| s.to_string_lossy().to_string()),
            AttributeValue::String(s) => Some(s.to_string_lossy().to_string()),
            AttributeValue::DebugStrOffsetsIndex(_index) => self
                .dwarf
                .attr_string(unit, *attr)
                .ok()
                .map(|s| s.to_string_lossy().to_string()),
            _ => None,
        }
    }

    fn get_byte_size(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<u64> {
        match entry.attr_value(gimli::DW_AT_byte_size).ok()?? {
            AttributeValue::Udata(size) => Some(size),
            AttributeValue::Data1(size) => Some(size as u64),
            AttributeValue::Data2(size) => Some(size as u64),
            AttributeValue::Data4(size) => Some(size as u64),
            AttributeValue::Data8(size) => Some(size),
            _ => None,
        }
    }

    fn get_encoding(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<gimli::DwAte> {
        match entry.attr_value(gimli::DW_AT_encoding).ok()?? {
            AttributeValue::Encoding(enc) => Some(enc),
            _ => None,
        }
    }

    /// Register a type under its global .debug_info offset as an alias.
    /// This allows lookups via DebugInfoRef to find types that were parsed
    /// with unit-local keys.
    fn register_global_alias(&mut self, local_key: DwarfTypeKey, global_offset: Option<usize>) {
        if let Some(global_off) = global_offset {
            // Get the TypeId that was registered for the local key
            if let Some(type_id) = self.type_table.get_by_dwarf_key(local_key) {
                // Register the same TypeId under the global key (using usize::MAX as sentinel)
                let global_key = GlobalTypeKey::global(global_off).to_dwarf_key();
                self.type_table.register_alias(global_key, type_id);
            }
        }
    }

    /// Get a type reference from DW_AT_type attribute.
    /// Returns a GlobalTypeKey that can handle both unit-local refs (UnitRef) and
    /// global refs (DebugInfoRef) which ARM compilers commonly use.
    fn get_type_ref_key(
        &self,
        unit_index: usize,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<GlobalTypeKey> {
        match entry.attr_value(gimli::DW_AT_type).ok()?? {
            // Unit-local reference (common in GCC)
            AttributeValue::UnitRef(offset) => Some(GlobalTypeKey::unit_local(
                unit_index,
                offset.0.into_u64() as usize,
            )),
            // Global reference in .debug_info (common in ARM compilers)
            AttributeValue::DebugInfoRef(offset) => {
                Some(GlobalTypeKey::global(offset.0.into_u64() as usize))
            }
            _ => None,
        }
    }

    /// Legacy helper - get type ref as UnitOffset (for backwards compatibility where needed)
    fn get_type_ref(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<UnitOffset> {
        match entry.attr_value(gimli::DW_AT_type).ok()?? {
            AttributeValue::UnitRef(offset) => Some(offset),
            _ => None,
        }
    }

    fn get_member_offset(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<u64> {
        match entry.attr_value(gimli::DW_AT_data_member_location).ok()?? {
            // Direct offset values (common in DWARF 4+, GCC)
            AttributeValue::Udata(offset) => Some(offset),
            AttributeValue::Data1(offset) => Some(offset as u64),
            AttributeValue::Data2(offset) => Some(offset as u64),
            AttributeValue::Data4(offset) => Some(offset as u64),
            AttributeValue::Data8(offset) => Some(offset),
            AttributeValue::Sdata(offset) => Some(offset as u64),
            // Expression-based offset (common in DWARF 3, ARM Compiler 5)
            // ARM Compiler uses DW_OP_plus_uconst expressions like "23 N" meaning offset N
            AttributeValue::Exprloc(expr) => self.evaluate_member_offset_expr(unit, &expr),
            // Block form (older DWARF)
            AttributeValue::Block(block) => {
                let expr = gimli::Expression(block);
                self.evaluate_member_offset_expr(unit, &expr)
            }
            _ => Some(0),
        }
    }

    /// Evaluate a member offset expression (typically DW_OP_plus_uconst)
    fn evaluate_member_offset_expr(
        &self,
        unit: &Unit<Reader<'a>>,
        expr: &gimli::Expression<Reader<'a>>,
    ) -> Option<u64> {
        let mut ops = expr.clone().operations(unit.encoding());

        // Look for DW_OP_plus_uconst which is the common case for member offsets
        // The expression is evaluated with an implicit "base address" on the stack,
        // and DW_OP_plus_uconst adds the offset to it.
        while let Ok(Some(op)) = ops.next() {
            match op {
                // DW_OP_plus_uconst: adds a constant to top of stack
                // For member offsets, this IS the offset value
                gimli::Operation::PlusConstant { value } => {
                    return Some(value);
                }
                // DW_OP_constu followed by DW_OP_plus
                gimli::Operation::UnsignedConstant { value } => {
                    // Check if next op is Plus
                    if let Ok(Some(gimli::Operation::Plus)) = ops.next() {
                        return Some(value);
                    }
                    // Otherwise this might be the offset directly
                    return Some(value);
                }
                // Direct address/literal (less common but possible)
                gimli::Operation::Address { address } => {
                    return Some(address);
                }
                _ => continue,
            }
        }

        // Default to 0 if we can't parse the expression
        Some(0)
    }

    fn get_bit_offset(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<u64> {
        // Try DW_AT_data_bit_offset first (DWARF 4+)
        if let Ok(Some(attr)) = entry.attr_value(gimli::DW_AT_data_bit_offset) {
            return match attr {
                AttributeValue::Udata(v) => Some(v),
                AttributeValue::Data1(v) => Some(v as u64),
                AttributeValue::Data2(v) => Some(v as u64),
                AttributeValue::Data4(v) => Some(v as u64),
                AttributeValue::Data8(v) => Some(v),
                _ => None,
            };
        }

        // Fall back to DW_AT_bit_offset (DWARF 3)
        match entry.attr_value(gimli::DW_AT_bit_offset).ok()?? {
            AttributeValue::Udata(v) => Some(v),
            AttributeValue::Data1(v) => Some(v as u64),
            AttributeValue::Data2(v) => Some(v as u64),
            AttributeValue::Data4(v) => Some(v as u64),
            AttributeValue::Data8(v) => Some(v),
            _ => None,
        }
    }

    fn get_bit_size(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<u64> {
        match entry.attr_value(gimli::DW_AT_bit_size).ok()?? {
            AttributeValue::Udata(v) => Some(v),
            AttributeValue::Data1(v) => Some(v as u64),
            AttributeValue::Data2(v) => Some(v as u64),
            AttributeValue::Data4(v) => Some(v as u64),
            AttributeValue::Data8(v) => Some(v),
            _ => None,
        }
    }

    fn get_const_value(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> Option<i64> {
        match entry.attr_value(gimli::DW_AT_const_value).ok()?? {
            AttributeValue::Sdata(v) => Some(v),
            AttributeValue::Udata(v) => Some(v as i64),
            AttributeValue::Data1(v) => Some(v as i64),
            AttributeValue::Data2(v) => Some(v as i64),
            AttributeValue::Data4(v) => Some(v as i64),
            AttributeValue::Data8(v) => Some(v as i64),
            _ => None,
        }
    }

    fn get_variable_location(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<u64> {
        let attr = entry.attr_value(gimli::DW_AT_location).ok()??;

        match attr {
            // Expression-based location (most common for global variables)
            AttributeValue::Exprloc(expr) => self.evaluate_simple_location_expr(unit, &expr),

            // Direct address value (some older DWARF or simple cases)
            AttributeValue::Addr(addr) => Some(addr),

            // Indexed address (DWARF 5)
            AttributeValue::DebugAddrIndex(index) => self.dwarf.address(unit, index).ok(),

            // Location list reference - need to evaluate the first entry
            // that covers address 0 (for static variables)
            AttributeValue::LocationListsRef(offset) => self.evaluate_location_list(unit, offset),

            // Offset into location lists (DWARF 5)
            AttributeValue::DebugLocListsIndex(index) => {
                if let Ok(offset) = self.dwarf.locations_offset(unit, index) {
                    self.evaluate_location_list(unit, offset)
                } else {
                    None
                }
            }

            // Block containing location expression (older DWARF)
            AttributeValue::Block(block) => {
                let expr = gimli::Expression(block);
                self.evaluate_simple_location_expr(unit, &expr)
            }

            // Data forms that might contain addresses directly
            AttributeValue::Udata(addr) => Some(addr),
            AttributeValue::Data1(addr) => Some(addr as u64),
            AttributeValue::Data2(addr) => Some(addr as u64),
            AttributeValue::Data4(addr) => Some(addr as u64),
            AttributeValue::Data8(addr) => Some(addr),
            AttributeValue::Sdata(addr) => Some(addr as u64),

            _ => None,
        }
    }

    /// Evaluate a location list to find the address for a global/static variable.
    /// For global variables, we look for an entry that covers "any" address or
    /// the first valid location expression.
    fn evaluate_location_list(
        &self,
        unit: &Unit<Reader<'a>>,
        offset: gimli::LocationListsOffset<<Reader<'a> as gimli::Reader>::Offset>,
    ) -> Option<u64> {
        let mut locations = self.dwarf.locations(unit, offset).ok()?;

        // Iterate through location list entries
        while let Ok(Some(entry)) = locations.next() {
            // Try to evaluate this location expression
            if let Some(addr) = self.evaluate_simple_location_expr(unit, &entry.data) {
                return Some(addr);
            }
        }
        None
    }

    /// Evaluate a DWARF location expression to extract a static address.
    ///
    /// This handles various forms that different compilers use:
    /// - DW_OP_addr: Direct address (GCC, Clang)
    /// - DW_OP_addrx: Indexed address in .debug_addr (DWARF 5, GCC 11+, Clang 14+)
    /// - DW_OP_GNU_addr_index: GNU extension for indexed addresses
    /// - DW_OP_constN + DW_OP_plus_uconst: Computed addresses (some ARM compilers)
    /// - Stack-based expressions with constants
    ///
    /// Note: We only handle static/global addresses. Register-relative or
    /// frame-relative locations (local variables) are not supported.
    fn evaluate_simple_location_expr(
        &self,
        unit: &Unit<Reader<'a>>,
        expr: &gimli::Expression<Reader<'a>>,
    ) -> Option<u64> {
        let mut ops = expr.clone().operations(unit.encoding());

        // Simple stack-based evaluation for common patterns
        let mut stack: Vec<u64> = Vec::new();

        while let Ok(Some(op)) = ops.next() {
            match op {
                // Direct address (DW_OP_addr) - most common for global variables
                gimli::Operation::Address { address } => {
                    // If this is the only operation or the final result, return it
                    stack.push(address);
                }

                // Indexed address (DW_OP_addrx, DW_OP_GNU_addr_index) - DWARF 5
                gimli::Operation::AddressIndex { index } => {
                    if let Ok(address) = self.dwarf.address(unit, index) {
                        stack.push(address);
                    }
                }

                // Unsigned constant (DW_OP_const1u, DW_OP_const2u, DW_OP_const4u, DW_OP_const8u, DW_OP_constu, DW_OP_lit*)
                gimli::Operation::UnsignedConstant { value } => {
                    stack.push(value);
                }

                // Signed constant (DW_OP_const1s, DW_OP_const2s, DW_OP_const4s, DW_OP_const8s, DW_OP_consts)
                gimli::Operation::SignedConstant { value } => {
                    stack.push(value as u64);
                }

                // Add constant to top of stack (DW_OP_plus_uconst)
                gimli::Operation::PlusConstant { value } => {
                    if let Some(top) = stack.pop() {
                        stack.push(top.wrapping_add(value));
                    }
                }

                // Add top two values (DW_OP_plus)
                gimli::Operation::Plus => {
                    if stack.len() >= 2 {
                        let a = stack.pop().unwrap();
                        let b = stack.pop().unwrap();
                        stack.push(a.wrapping_add(b));
                    }
                }

                // Implicit pointer or value - these indicate the value is not in memory
                // at a simple address, so we can't handle them
                gimli::Operation::ImplicitValue { .. }
                | gimli::Operation::ImplicitPointer { .. }
                | gimli::Operation::StackValue => {
                    // These mean the value is computed/optimized, not at a memory address
                    return None;
                }

                // Register-based locations - local variables, can't handle statically
                gimli::Operation::Register { .. }
                | gimli::Operation::RegisterOffset { .. }
                | gimli::Operation::FrameOffset { .. }
                | gimli::Operation::CallFrameCFA => {
                    // These are for local variables on stack/registers
                    return None;
                }

                // Piece operations - composite locations
                gimli::Operation::Piece { .. } => {
                    // Composite location - take what we have so far if valid
                    break;
                }

                // Other operations we don't handle - continue to see if there's an address later
                _ => continue,
            }
        }

        // Return the top of stack if we have something
        stack.pop()
    }

    fn get_virtuality(
        &self,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<gimli::DwVirtuality> {
        match entry.attr_value(gimli::DW_AT_virtuality).ok()?? {
            AttributeValue::Virtuality(v) => Some(v),
            _ => None,
        }
    }

    fn has_declaration_attr(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> bool {
        matches!(
            entry.attr_value(gimli::DW_AT_declaration).ok(),
            Some(Some(AttributeValue::Flag(true)))
        )
    }

    fn has_external_attr(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> bool {
        matches!(
            entry.attr_value(gimli::DW_AT_external).ok(),
            Some(Some(AttributeValue::Flag(true)))
        )
    }

    fn has_enum_class_attr(&self, entry: &DebuggingInformationEntry<Reader<'a>>) -> bool {
        matches!(
            entry.attr_value(gimli::DW_AT_enum_class).ok(),
            Some(Some(AttributeValue::Flag(true)))
        )
    }

    fn get_array_count(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<u64> {
        // Look for DW_TAG_subrange_type child with count or upper_bound
        if let Ok(mut tree) = unit.entries_tree(Some(entry.offset())) {
            if let Ok(root) = tree.root() {
                let mut children = root.children();
                while let Ok(Some(child)) = children.next() {
                    if child.entry().tag() == gimli::DW_TAG_subrange_type {
                        // Try DW_AT_count first
                        if let Ok(Some(attr)) = child.entry().attr_value(gimli::DW_AT_count) {
                            return match attr {
                                AttributeValue::Udata(v) => Some(v),
                                AttributeValue::Data1(v) => Some(v as u64),
                                AttributeValue::Data2(v) => Some(v as u64),
                                AttributeValue::Data4(v) => Some(v as u64),
                                AttributeValue::Data8(v) => Some(v),
                                _ => None,
                            };
                        }
                        // Try DW_AT_upper_bound (count = upper_bound + 1)
                        if let Ok(Some(attr)) = child.entry().attr_value(gimli::DW_AT_upper_bound) {
                            return match attr {
                                AttributeValue::Udata(v) => Some(v + 1),
                                AttributeValue::Data1(v) => Some(v as u64 + 1),
                                AttributeValue::Data2(v) => Some(v as u64 + 1),
                                AttributeValue::Data4(v) => Some(v as u64 + 1),
                                AttributeValue::Data8(v) => Some(v + 1),
                                AttributeValue::Sdata(v) => Some((v + 1) as u64),
                                _ => None,
                            };
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dwarf_type_key() {
        let key1 = DwarfTypeKey::new(0, 100);
        let key2 = DwarfTypeKey::new(0, 100);
        let key3 = DwarfTypeKey::new(1, 100);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    // Note: More comprehensive tests require actual ELF files with DWARF info.
    // Those would be integration tests in a separate test file.
}
