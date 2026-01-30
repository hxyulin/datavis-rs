//! Quick DWARF analysis tool to debug AXF/ELF parsing issues
//!
//! Run with: cargo run --example analyze_dwarf -- <axf-file>

#![allow(deprecated)] // Example code may use deprecated APIs for demonstration

use gimli::{AttributeValue, EndianSlice, RunTimeEndian};
use object::{Object, ObjectSection};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs;

/// Information about a DIE we've seen
#[derive(Debug, Clone)]
struct DieInfo {
    tag: gimli::DwTag,
    name: Option<String>,
    has_type: bool,
    has_location: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <axf-file>", args[0]);
        std::process::exit(1);
    }

    let data = fs::read(&args[1])?;
    println!("Analyzing: {}", args[1]);
    println!("File size: {} bytes", data.len());
    println!();

    let file = object::File::parse(&*data)?;
    let endian = if file.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };

    let load_section = |id: gimli::SectionId| -> Result<Cow<[u8]>, gimli::Error> {
        Ok(file
            .section_by_name(id.name())
            .and_then(|s| s.data().ok())
            .map(Cow::Borrowed)
            .unwrap_or(Cow::Borrowed(&[])))
    };

    let dwarf_cow = gimli::Dwarf::load(load_section)?;
    let dwarf = dwarf_cow.borrow(|section| EndianSlice::new(section, endian));

    // First pass: collect information about ALL DIEs (not just variables)
    // This lets us understand what abstract_origin references point to
    let mut all_dies: HashMap<usize, DieInfo> = HashMap::new();

    let mut units = dwarf.units();
    while let Ok(Some(header)) = units.next() {
        let unit = dwarf.unit(header)?;
        let mut entries = unit.entries();

        while let Ok(Some((_, entry))) = entries.next_dfs() {
            let global_offset = entry
                .offset()
                .to_debug_info_offset(&unit.header)
                .map(|o| o.0);

            if let Some(off) = global_offset {
                let name =
                    entry
                        .attr_value(gimli::DW_AT_name)
                        .ok()
                        .flatten()
                        .and_then(|v| match v {
                            AttributeValue::String(s) => s.to_string().ok().map(|s| s.to_string()),
                            AttributeValue::DebugStrRef(offset) => dwarf
                                .string(offset)
                                .ok()
                                .and_then(|s| s.to_string().ok().map(|s| s.to_string())),
                            _ => None,
                        });

                all_dies.insert(
                    off,
                    DieInfo {
                        tag: entry.tag(),
                        name,
                        has_type: entry.attr_value(gimli::DW_AT_type).ok().flatten().is_some(),
                        has_location: entry
                            .attr_value(gimli::DW_AT_location)
                            .ok()
                            .flatten()
                            .is_some(),
                    },
                );
            }
        }
    }

    println!("First pass: collected {} DIE entries", all_dies.len());
    println!();

    // Second pass: analyze variables specifically
    let mut units = dwarf.units();

    // Statistics
    let mut total_vars = 0;
    let mut vars_with_declaration = 0;
    let mut vars_with_specification = 0;
    let mut vars_with_abstract_origin = 0;
    let mut vars_with_type = 0;
    let mut vars_with_location = 0;
    let mut vars_with_name = 0;

    // Track declarations and specifications for matching analysis
    let mut declarations: HashMap<usize, String> = HashMap::new(); // offset -> name
    let mut specifications: Vec<(usize, Option<String>)> = Vec::new(); // (target_offset, own_name)
    let mut abstract_origins: Vec<(usize, Option<String>)> = Vec::new(); // (target_offset, own_name)

    while let Ok(Some(header)) = units.next() {
        let unit = dwarf.unit(header)?;
        let mut entries = unit.entries();

        while let Ok(Some((_, entry))) = entries.next_dfs() {
            if entry.tag() != gimli::DW_TAG_variable {
                continue;
            }

            total_vars += 1;

            // Get global offset for this DIE
            let global_offset = entry
                .offset()
                .to_debug_info_offset(&unit.header)
                .map(|o| o.0);

            // Check attributes
            let has_declaration = matches!(
                entry.attr_value(gimli::DW_AT_declaration).ok(),
                Some(Some(AttributeValue::Flag(true)))
            );

            let has_type = entry.attr_value(gimli::DW_AT_type).ok().flatten().is_some();
            let has_location = entry
                .attr_value(gimli::DW_AT_location)
                .ok()
                .flatten()
                .is_some();

            // Get name
            let name = entry
                .attr_value(gimli::DW_AT_name)
                .ok()
                .flatten()
                .and_then(|v| match v {
                    AttributeValue::String(s) => s.to_string().ok().map(|s| s.to_string()),
                    AttributeValue::DebugStrRef(offset) => dwarf
                        .string(offset)
                        .ok()
                        .and_then(|s| s.to_string().ok().map(|s| s.to_string())),
                    _ => None,
                });

            // Check for specification/origin
            let spec_target = entry
                .attr_value(gimli::DW_AT_specification)
                .ok()
                .flatten()
                .and_then(|v| match v {
                    AttributeValue::UnitRef(offset) => {
                        offset.to_debug_info_offset(&unit.header).map(|o| o.0)
                    }
                    AttributeValue::DebugInfoRef(offset) => Some(offset.0),
                    _ => None,
                });

            let origin_target = entry
                .attr_value(gimli::DW_AT_abstract_origin)
                .ok()
                .flatten()
                .and_then(|v| match v {
                    AttributeValue::UnitRef(offset) => {
                        offset.to_debug_info_offset(&unit.header).map(|o| o.0)
                    }
                    AttributeValue::DebugInfoRef(offset) => Some(offset.0),
                    _ => None,
                });

            // Update statistics
            if has_declaration {
                vars_with_declaration += 1;
            }
            if spec_target.is_some() {
                vars_with_specification += 1;
            }
            if origin_target.is_some() {
                vars_with_abstract_origin += 1;
            }
            if has_type {
                vars_with_type += 1;
            }
            if has_location {
                vars_with_location += 1;
            }
            if name.is_some() {
                vars_with_name += 1;
            }

            // Track for matching
            if has_declaration {
                if let Some(off) = global_offset {
                    declarations
                        .insert(off, name.clone().unwrap_or_else(|| "<unnamed>".to_string()));
                }
            }
            if let Some(target) = spec_target {
                specifications.push((target, name.clone()));
            }
            if let Some(target) = origin_target {
                abstract_origins.push((target, name.clone()));
            }
        }
    }

    println!("=== DW_TAG_variable Statistics ===");
    println!("Total variables:              {}", total_vars);
    println!(
        "  With DW_AT_declaration:     {} (declarations that will be targets)",
        vars_with_declaration
    );
    println!(
        "  With DW_AT_specification:   {} (should reference declarations)",
        vars_with_specification
    );
    println!(
        "  With DW_AT_abstract_origin: {} (for inlined variables)",
        vars_with_abstract_origin
    );
    println!(
        "  With DW_AT_type:            {} (have type info)",
        vars_with_type
    );
    println!(
        "  With DW_AT_location:        {} (have address)",
        vars_with_location
    );
    println!(
        "  With DW_AT_name:            {} (have name)",
        vars_with_name
    );
    println!();

    // Analyze specification matching
    let mut spec_matched = 0;
    let mut spec_unmatched = 0;

    for (target, _) in &specifications {
        if declarations.contains_key(target) {
            spec_matched += 1;
        } else {
            spec_unmatched += 1;
        }
    }

    println!("=== Specification Matching ===");
    println!("Declarations available:       {}", declarations.len());
    println!("Specifications total:         {}", specifications.len());
    println!("  Matched to declaration:     {}", spec_matched);
    println!("  Unmatched:                  {}", spec_unmatched);

    // Analyze abstract_origin - these typically point to formal_parameter or variable in abstract functions
    println!();
    println!("=== Abstract Origin Analysis ===");
    println!("Total abstract_origin refs:   {}", abstract_origins.len());

    // Count what types of DIEs the abstract_origins point to
    let mut origin_target_tags: HashMap<String, usize> = HashMap::new();
    let mut origin_with_type = 0;
    let mut origin_with_location = 0;
    let mut origin_not_found = 0;

    for (target, _) in &abstract_origins {
        if let Some(die_info) = all_dies.get(target) {
            let tag_name = format!("{}", die_info.tag);
            *origin_target_tags.entry(tag_name).or_insert(0) += 1;
            if die_info.has_type {
                origin_with_type += 1;
            }
            if die_info.has_location {
                origin_with_location += 1;
            }
        } else {
            origin_not_found += 1;
        }
    }

    println!("  Target DIE not found:       {}", origin_not_found);
    println!("  Targets with DW_AT_type:    {}", origin_with_type);
    println!("  Targets with DW_AT_location: {}", origin_with_location);
    println!();
    println!("  Target DIE types:");
    let mut sorted_tags: Vec<_> = origin_target_tags.iter().collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(a.1));
    for (tag, count) in sorted_tags {
        println!("    {}: {}", tag, count);
    }

    // Show sample abstract_origin targets with their info
    println!();
    println!("  Sample abstract_origin targets:");
    for (target, own_name) in abstract_origins.iter().take(5) {
        if let Some(die_info) = all_dies.get(target) {
            println!(
                "    0x{:x} -> {} {:?} (has_type={}, has_loc={})",
                target, die_info.tag, die_info.name, die_info.has_type, die_info.has_location
            );
        } else {
            println!("    0x{:x} -> NOT FOUND (own_name={:?})", target, own_name);
        }
    }

    println!();
    println!("=== Analysis Summary ===");
    println!(
        "Variables with addresses (DW_AT_location): {}",
        vars_with_location
    );
    println!(
        "Variables that are pure declarations:      {}",
        vars_with_declaration
    );
    println!(
        "Variables with abstract_origin:            {}",
        vars_with_abstract_origin
    );
    println!(
        "  (these are typically inlined function locals - often can't be statically resolved)"
    );

    // Variables without type but with location (the problematic ones)
    println!();
    println!("Potential issues:");
    let no_type_estimate = total_vars - vars_with_type;
    if no_type_estimate > 0 {
        println!("  - {} variables without DW_AT_type", no_type_estimate);
    }
    if spec_unmatched > 0 {
        println!(
            "  - {} specifications reference non-existent declarations",
            spec_unmatched
        );
    }

    // The key insight: abstract_origin variables are inlined locals
    // They typically reference formal_parameter or variable DIEs in the abstract function
    // These usually have register-based or frame-relative locations, not static addresses
    println!();
    println!("=== Conclusion ===");
    let static_vars = vars_with_location - vars_with_abstract_origin;
    println!(
        "Likely static/global variables: {} (have location but no abstract_origin)",
        static_vars
    );
    println!(
        "Likely inlined locals:          {} (have abstract_origin)",
        vars_with_abstract_origin
    );
    println!();
    println!(
        "The {} variables with abstract_origin are typically:",
        vars_with_abstract_origin
    );
    println!("  - Local variables from inlined functions");
    println!("  - Their locations are often register-based or frame-relative");
    println!("  - Cannot be resolved to static addresses");

    Ok(())
}
